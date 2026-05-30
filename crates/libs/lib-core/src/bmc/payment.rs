use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::model::payment::{PaymentAttempt, PaymentStatus};
use crate::Result;

// region:    --- PaymentBmc

pub struct PaymentBmc;

impl PaymentBmc {
    /// Check for existing payment attempt with given idempotency key + invoice
    pub async fn get_by_idempotency_key(
        pool: &PgPool,
        invoice_id: Uuid,
        idempotency_key: &str,
    ) -> Result<Option<PaymentAttempt>> {
        let attempt = sqlx::query_as::<_, PaymentAttempt>(
            "SELECT * FROM payment_attempts WHERE idempotency_key = $1 AND invoice_id = $2",
        )
        .bind(idempotency_key)
        .bind(invoice_id)
        .fetch_optional(pool)
        .await?;

        Ok(attempt)
    }

    /// Check if there is already a pending or succeeded payment attempt for this invoice.
    /// Must be called within the same transaction that holds the FOR UPDATE lock.
    /// This prevents concurrent requests from all creating payment attempts.
    pub async fn has_active_attempt_tx(
        tx: &mut Transaction<'static, Postgres>,
        invoice_id: Uuid,
    ) -> Result<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1 FROM payment_attempts
                WHERE invoice_id = $1 AND status IN ('pending', 'succeeded')
            )",
        )
        .bind(invoice_id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(exists)
    }

    /// Create a new payment attempt in pending state within an existing transaction.
    /// This ensures the row lock from get_for_payment_tx is still held.
    ///
    /// IMPORTANT: Call `has_active_attempt_tx` first to prevent double-charges.
    pub async fn create_pending_tx(
        tx: &mut Transaction<'static, Postgres>,
        invoice_id: Uuid,
        idempotency_key: &str,
        amount_cents: i64,
        card_token: &str,
    ) -> Result<PaymentAttempt> {
        let attempt = sqlx::query_as::<_, PaymentAttempt>(
            "INSERT INTO payment_attempts (id, invoice_id, idempotency_key, status, amount_cents, card_token, created_at, updated_at)
             VALUES ($1, $2, $3, 'pending', $4, $5, NOW(), NOW())
             RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(invoice_id)
        .bind(idempotency_key)
        .bind(amount_cents)
        .bind(card_token)
        .fetch_one(&mut **tx)
        .await?;

        Ok(attempt)
    }

    /// Mark payment as succeeded with PSP reference
    pub async fn mark_succeeded(
        pool: &PgPool,
        id: Uuid,
        psp_ref: &Option<String>,
    ) -> Result<PaymentAttempt> {
        let attempt = sqlx::query_as::<_, PaymentAttempt>(
            "UPDATE payment_attempts SET status = 'succeeded', psp_ref = $2, updated_at = NOW()
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(psp_ref)
        .fetch_one(pool)
        .await?;

        Ok(attempt)
    }

    /// Mark payment as succeeded within an existing transaction
    pub async fn mark_succeeded_tx(
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        id: Uuid,
        psp_ref: &Option<String>,
    ) -> Result<PaymentAttempt> {
        let attempt = sqlx::query_as::<_, PaymentAttempt>(
            "UPDATE payment_attempts SET status = 'succeeded', psp_ref = $2, updated_at = NOW()
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(psp_ref)
        .fetch_one(&mut **tx)
        .await?;

        Ok(attempt)
    }

    /// Mark payment as failed with failure code
    pub async fn mark_failed(
        pool: &PgPool,
        id: Uuid,
        failure_code: &Option<String>,
    ) -> Result<PaymentAttempt> {
        let attempt = sqlx::query_as::<_, PaymentAttempt>(
            "UPDATE payment_attempts SET status = 'failed', failure_code = $2, updated_at = NOW()
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(failure_code)
        .fetch_one(pool)
        .await?;

        Ok(attempt)
    }

    /// Mark payment as failed within an existing transaction
    pub async fn mark_failed_tx(
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        id: Uuid,
        failure_code: &Option<String>,
    ) -> Result<PaymentAttempt> {
        let attempt = sqlx::query_as::<_, PaymentAttempt>(
            "UPDATE payment_attempts SET status = 'failed', failure_code = $2, updated_at = NOW()
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(failure_code)
        .fetch_one(&mut **tx)
        .await?;

        Ok(attempt)
    }

    /// Get a pending payment attempt response (for timeout/error cases)
    pub fn pending_response(attempt: &PaymentAttempt) -> PaymentAttempt {
        PaymentAttempt {
            id: attempt.id,
            invoice_id: attempt.invoice_id,
            idempotency_key: attempt.idempotency_key.clone(),
            status: PaymentStatus::Pending,
            amount_cents: attempt.amount_cents,
            card_token: attempt.card_token.clone(),
            psp_ref: None,
            failure_code: Some("psp_unavailable".to_string()),
            created_at: attempt.created_at,
            updated_at: attempt.updated_at,
        }
    }
}

// endregion: --- PaymentBmc
