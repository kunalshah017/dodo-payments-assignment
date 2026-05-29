use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use uuid::Uuid;

use lib_core::bmc::invoice::InvoiceBmc;
use lib_core::bmc::payment::PaymentBmc;
use lib_core::ctx::Ctx;
use lib_core::error::ErrorBody;
use lib_core::model::invoice::InvoiceStatus;
use lib_core::model::payment::*;
use lib_core::{Error, Result};

use crate::routes::AppState;
use crate::services::{psp_client, webhook_dispatcher};

#[utoipa::path(
    post,
    path = "/v1/invoices/{id}/pay",
    params(("id" = Uuid, Path, description = "Invoice UUID")),
    request_body = PayInvoiceRequest,
    responses(
        (status = 200, description = "Payment succeeded or failed", body = PaymentAttemptResponse),
        (status = 202, description = "Payment pending (PSP timeout)", body = PaymentAttemptResponse),
        (status = 400, description = "Missing idempotency key", body = ErrorBody),
        (status = 404, description = "Invoice not found", body = ErrorBody),
        (status = 409, description = "Invoice not in open state or idempotency conflict", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Payments"
)]
pub async fn pay_invoice(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Path(invoice_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<PayInvoiceRequest>,
) -> Result<(StatusCode, Json<PaymentAttemptResponse>)> {
    // Require idempotency key
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::BadRequest("Idempotency-Key header is required".to_string()))?
        .to_string();

    // Validate idempotency key (non-empty, max 256 chars)
    if idempotency_key.trim().is_empty() || idempotency_key.len() > 256 {
        return Err(Error::BadRequest(
            "Idempotency-Key must be 1-256 characters".to_string(),
        ));
    }

    // Validate card token
    if req.card_token.trim().is_empty() {
        return Err(Error::BadRequest("card_token is required".to_string()));
    }

    // Check for existing payment with this idempotency key (fast path, outside tx)
    if let Some(existing) =
        PaymentBmc::get_by_idempotency_key(&state.db, invoice_id, &idempotency_key).await?
    {
        if existing.card_token != req.card_token {
            return Err(Error::Conflict(
                "Idempotency key already used with different request body".to_string(),
            ));
        }
        return Ok((StatusCode::OK, Json(existing.into())));
    }

    // Begin transaction: lock invoice row + create payment attempt atomically.
    // This prevents concurrent double-charges — the second request will block
    // on FOR UPDATE until the first transaction commits.
    let (invoice, mut tx) =
        InvoiceBmc::get_for_payment_tx(&state.db, &ctx, invoice_id).await?;

    if invoice.status != InvoiceStatus::Open {
        return Err(Error::Conflict(format!(
            "Cannot pay invoice in {:?} state. Invoice must be in 'open' state.",
            invoice.status
        )));
    }

    // Check if there's already an active (pending/succeeded) payment attempt.
    // Since we hold the FOR UPDATE lock, no other transaction can slip in between.
    if PaymentBmc::has_active_attempt_tx(&mut tx, invoice_id).await? {
        return Err(Error::Conflict(
            "A payment is already in progress or completed for this invoice".to_string(),
        ));
    }

    // Create payment attempt within the transaction (lock still held)
    let attempt = PaymentBmc::create_pending_tx(
        &mut tx,
        invoice_id,
        &idempotency_key,
        invoice.total_amount_cents,
        &req.card_token,
    )
    .await?;

    // Commit: releases the row lock. The pending payment is now visible.
    // If a concurrent request arrives, it will see this pending attempt via
    // the has_active_attempt_tx check, or the invoice will already be paid.
    tx.commit().await?;

    // Call the PSP with a timeout (lock is released, no DB resources held)
    let psp_result = psp_client::charge(
        &state.psp_base_url,
        &req.card_token,
        invoice.total_amount_cents,
    )
    .await;

    // Update payment attempt and invoice based on PSP result
    match psp_result {
        Ok(psp_response) if psp_response.status == "succeeded" => {
            let updated =
                PaymentBmc::mark_succeeded(&state.db, attempt.id, &psp_response.psp_ref).await?;

            InvoiceBmc::mark_paid(&state.db, invoice_id).await?;

            // Dispatch webhook
            webhook_dispatcher::dispatch_event(
                &state.db,
                ctx.business_id(),
                lib_core::model::webhook::WebhookEventType::InvoicePaid,
                &invoice,
            )
            .await;

            Ok((StatusCode::OK, Json(updated.into())))
        }
        Ok(psp_response) => {
            // PSP returned a definitive failure (declined, invalid token, etc.)
            let updated =
                PaymentBmc::mark_failed(&state.db, attempt.id, &psp_response.code).await?;

            webhook_dispatcher::dispatch_event(
                &state.db,
                ctx.business_id(),
                lib_core::model::webhook::WebhookEventType::InvoicePaymentFailed,
                &invoice,
            )
            .await;

            Ok((StatusCode::OK, Json(updated.into())))
        }
        Err(psp_client::PspError::ClientError(status_code)) => {
            // PSP rejected the request (4xx) — this is a terminal failure
            let error_code = format!("psp_rejected_{}", status_code);
            let updated =
                PaymentBmc::mark_failed(&state.db, attempt.id, &Some(error_code)).await?;

            webhook_dispatcher::dispatch_event(
                &state.db,
                ctx.business_id(),
                lib_core::model::webhook::WebhookEventType::InvoicePaymentFailed,
                &invoice,
            )
            .await;

            Ok((StatusCode::OK, Json(updated.into())))
        }
        Err(e) => {
            // PSP timeout or server error — leave payment in pending state (retryable)
            tracing::error!(error = %e, "PSP call failed for payment attempt {}", attempt.id);

            let pending = PaymentBmc::pending_response(&attempt);
            Ok((StatusCode::ACCEPTED, Json(pending.into())))
        }
    }
}
