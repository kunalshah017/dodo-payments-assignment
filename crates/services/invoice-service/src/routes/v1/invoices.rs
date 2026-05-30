use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use uuid::Uuid;

use lib_core::bmc::invoice::InvoiceBmc;
use lib_core::ctx::Ctx;
use lib_core::error::ErrorBody;
use lib_core::model::invoice::*;
use lib_core::{Error, Result};

use crate::routes::AppState;
use crate::services::webhook_dispatcher;

// region:    --- Helpers

fn invoice_to_response(invoice: Invoice, line_items: Vec<LineItem>) -> InvoiceResponse {
    InvoiceResponse {
        id: invoice.id,
        customer_id: invoice.customer_id,
        status: invoice.status,
        total_amount_cents: invoice.total_amount_cents,
        due_date: invoice.due_date,
        line_items: line_items.into_iter().map(Into::into).collect(),
        created_at: invoice.created_at,
    }
}

// endregion: --- Helpers

#[utoipa::path(
    post,
    path = "/v1/invoices",
    request_body = InvoiceCreate,
    responses(
        (status = 201, description = "Invoice created", body = InvoiceResponse),
        (status = 400, description = "Validation error", body = ErrorBody),
        (status = 404, description = "Customer not found", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Invoices"
)]
pub async fn create_invoice(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Json(req): Json<InvoiceCreate>,
) -> Result<(StatusCode, Json<InvoiceResponse>)> {
    // Validate line items
    if req.line_items.is_empty() {
        return Err(Error::BadRequest(
            "At least one line item is required".to_string(),
        ));
    }
    for item in &req.line_items {
        if item.quantity <= 0 {
            return Err(Error::BadRequest(
                "Line item quantity must be positive".to_string(),
            ));
        }
        if item.unit_amount_cents < 0 {
            return Err(Error::BadRequest(
                "Line item unit_amount_cents must be non-negative".to_string(),
            ));
        }
        if item.description.trim().is_empty() {
            return Err(Error::BadRequest(
                "Line item description is required".to_string(),
            ));
        }
    }

    // Verify customer belongs to this business
    if !InvoiceBmc::validate_customer(&state.db, &ctx, req.customer_id).await? {
        return Err(Error::NotFound(format!(
            "Customer {} not found",
            req.customer_id
        )));
    }

    let (invoice, line_items) = InvoiceBmc::create(&state.db, &ctx, req).await?;

    // Enqueue webhook in a separate transaction (invoice is already committed).
    // For invoice.created, the risk of lost events on crash between create commit
    // and this point is acceptable — the retry worker and event listing API provide
    // reconciliation. The critical path (payments) uses full transactional outbox.
    {
        let mut tx = state.db.begin().await?;
        let payload = webhook_dispatcher::build_event_payload(
            &lib_core::model::webhook::WebhookEventType::InvoiceCreated,
            &invoice,
        );
        let event_ids = webhook_dispatcher::enqueue_events_tx(
            &mut tx,
            ctx.business_id(),
            lib_core::model::webhook::WebhookEventType::InvoiceCreated,
            &payload,
        )
        .await;
        tx.commit().await?;
        webhook_dispatcher::spawn_delivery(&state.db, event_ids);
    }

    Ok((
        StatusCode::CREATED,
        Json(invoice_to_response(invoice, line_items)),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/invoices/{id}",
    params(("id" = Uuid, Path, description = "Invoice UUID")),
    responses(
        (status = 200, description = "Invoice found", body = InvoiceResponse),
        (status = 404, description = "Invoice not found", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Invoices"
)]
pub async fn get_invoice(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Path(id): Path<Uuid>,
) -> Result<Json<InvoiceResponse>> {
    let (invoice, line_items) = InvoiceBmc::get(&state.db, &ctx, id).await?;
    Ok(Json(invoice_to_response(invoice, line_items)))
}

#[utoipa::path(
    get,
    path = "/v1/invoices",
    params(("status" = Option<InvoiceStatus>, Query, description = "Filter by invoice status")),
    responses(
        (status = 200, description = "List of invoices", body = Vec<InvoiceResponse>),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Invoices"
)]
pub async fn list_invoices(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Query(params): Query<InvoiceListQuery>,
) -> Result<Json<Vec<InvoiceResponse>>> {
    let results = InvoiceBmc::list(&state.db, &ctx, params).await?;
    let response: Vec<InvoiceResponse> = results
        .into_iter()
        .map(|(inv, lis)| invoice_to_response(inv, lis))
        .collect();
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/v1/invoices/{id}/finalize",
    params(("id" = Uuid, Path, description = "Invoice UUID")),
    responses(
        (status = 200, description = "Invoice finalized", body = InvoiceResponse),
        (status = 404, description = "Invoice not found", body = ErrorBody),
        (status = 409, description = "Invalid state transition", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Invoices"
)]
pub async fn finalize_invoice(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Path(id): Path<Uuid>,
) -> Result<Json<InvoiceResponse>> {
    let (invoice, line_items) = InvoiceBmc::finalize(&state.db, &ctx, id).await?;
    Ok(Json(invoice_to_response(invoice, line_items)))
}

#[utoipa::path(
    post,
    path = "/v1/invoices/{id}/void",
    params(("id" = Uuid, Path, description = "Invoice UUID")),
    responses(
        (status = 200, description = "Invoice voided", body = InvoiceResponse),
        (status = 404, description = "Invoice not found", body = ErrorBody),
        (status = 409, description = "Invalid state transition", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Invoices"
)]
pub async fn void_invoice(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Path(id): Path<Uuid>,
) -> Result<Json<InvoiceResponse>> {
    let (invoice, line_items) = InvoiceBmc::void(&state.db, &ctx, id).await?;
    Ok(Json(invoice_to_response(invoice, line_items)))
}

#[utoipa::path(
    post,
    path = "/v1/invoices/{id}/mark-uncollectible",
    params(("id" = Uuid, Path, description = "Invoice UUID")),
    responses(
        (status = 200, description = "Invoice marked uncollectible", body = InvoiceResponse),
        (status = 404, description = "Invoice not found", body = ErrorBody),
        (status = 409, description = "Invalid state transition", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Invoices"
)]
pub async fn mark_uncollectible(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Path(id): Path<Uuid>,
) -> Result<Json<InvoiceResponse>> {
    let (invoice, line_items) = InvoiceBmc::mark_uncollectible(&state.db, &ctx, id).await?;
    Ok(Json(invoice_to_response(invoice, line_items)))
}
