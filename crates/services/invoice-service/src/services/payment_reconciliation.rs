use sqlx::PgPool;
use std::time::Duration;

// region:    --- Payment Reconciliation Worker

/// Background worker that expires stale pending payment attempts.
///
/// Production payment gateways (Razorpay, Stripe) reconcile pending payments
/// by polling the PSP's status endpoint. Since our mock PSP has no such endpoint,
/// we use a TTL-based approach: payments pending longer than 10 minutes are
/// marked as `failed` with code `payment_expired`.
///
/// This unblocks the invoice for new payment attempts — the same behavior as
/// Razorpay's "polling acquiring banks periodically" pattern.
///
/// In production, this worker would:
/// 1. Query the PSP's `/charges/{psp_ref}` endpoint for the real status
/// 2. Mark as succeeded/failed based on the PSP's authoritative answer
/// 3. Only expire after exhausting PSP queries (e.g., 3 attempts over 10 min)
pub fn start_worker(pool: PgPool) {
    tokio::spawn(async move {
        tracing::info!("Payment reconciliation worker started (10-min TTL for pending payments)");
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            if let Err(e) = expire_stale_payments(&pool).await {
                tracing::error!(error = %e, "Payment reconciliation worker error");
            }
        }
    });
}

/// Expire pending payments older than 10 minutes.
/// These are payments where the PSP timed out or returned a network error,
/// and no resolution has arrived within the reconciliation window.
async fn expire_stale_payments(pool: &PgPool) -> anyhow::Result<()> {
    let expired = sqlx::query_scalar::<_, i64>(
        "WITH expired AS (
            UPDATE payment_attempts
            SET status = 'failed',
                failure_code = 'payment_expired',
                updated_at = NOW()
            WHERE status = 'pending'
              AND created_at < NOW() - INTERVAL '10 minutes'
            RETURNING 1
        )
        SELECT COUNT(*) FROM expired",
    )
    .fetch_one(pool)
    .await?;

    if expired > 0 {
        tracing::info!(
            count = expired,
            "Expired stale pending payments (>10 min without resolution)"
        );
    }

    Ok(())
}

// endregion: --- Payment Reconciliation Worker
