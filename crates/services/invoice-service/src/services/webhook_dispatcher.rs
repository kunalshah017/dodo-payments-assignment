use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use lib_core::model::invoice::Invoice;
use lib_core::model::webhook::{WebhookEndpoint, WebhookEventType};

type HmacSha256 = Hmac<Sha256>;

// region:    --- Transactional Outbox

/// Build the webhook event payload for a given invoice state change.
/// Called inside the same transaction as the state change so the invoice
/// data is guaranteed to reflect the committed state.
pub fn build_event_payload(event_type: &WebhookEventType, invoice: &Invoice) -> serde_json::Value {
    serde_json::json!({
        "event_id": Uuid::new_v4().to_string(),
        "event_type": event_type,
        "data": {
            "invoice_id": invoice.id,
            "status": invoice.status,
            "total_amount_cents": invoice.total_amount_cents,
            "customer_id": invoice.customer_id,
        },
        "created_at": chrono::Utc::now().to_rfc3339(),
    })
}

/// Enqueue webhook events transactionally. Must be called within the same
/// transaction that performs the state change. This guarantees:
/// 1. If the transaction commits, events are durably stored (no lost events on crash)
/// 2. The payload captures the exact point-in-time state (no stale/future data)
/// 3. Delivery is fully decoupled — can retry indefinitely from persisted events
///
/// Returns the IDs of enqueued events for immediate delivery attempt.
pub async fn enqueue_events_tx(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    business_id: Uuid,
    event_type: WebhookEventType,
    payload: &serde_json::Value,
) -> Vec<Uuid> {
    let endpoints = match sqlx::query_as::<_, WebhookEndpoint>(
        "SELECT * FROM webhook_endpoints WHERE business_id = $1",
    )
    .bind(business_id)
    .fetch_all(&mut **tx)
    .await
    {
        Ok(e) => e,
        Err(err) => {
            tracing::error!(error = %err, "Failed to fetch webhook endpoints in tx");
            return vec![];
        }
    };

    let mut event_ids = Vec::with_capacity(endpoints.len());

    for endpoint in &endpoints {
        let event_id = Uuid::new_v4();
        if let Err(e) = sqlx::query(
            "INSERT INTO webhook_events (id, endpoint_id, event_type, payload, attempts, created_at)
             VALUES ($1, $2, $3, $4, 0, NOW())",
        )
        .bind(event_id)
        .bind(endpoint.id)
        .bind(&event_type)
        .bind(payload)
        .execute(&mut **tx)
        .await
        {
            tracing::error!(error = %e, endpoint_id = %endpoint.id, "Failed to enqueue webhook event");
            continue;
        }
        event_ids.push(event_id);
    }

    event_ids
}

// endregion: --- Transactional Outbox

// region:    --- Delivery (fire-and-forget)

/// Spawn async delivery for already-persisted webhook events.
/// This is best-effort — if delivery fails, the retry worker will pick it up.
/// If the process crashes before delivery, events are already in the DB.
pub fn spawn_delivery(pool: &PgPool, event_ids: Vec<Uuid>) {
    if event_ids.is_empty() {
        return;
    }

    let pool = pool.clone();
    tokio::spawn(async move {
        for event_id in event_ids {
            // Fetch the persisted event + endpoint for delivery
            let row = sqlx::query(
                "SELECT we.url, we.secret, wev.payload
                 FROM webhook_events wev
                 JOIN webhook_endpoints we ON we.id = wev.endpoint_id
                 WHERE wev.id = $1",
            )
            .bind(event_id)
            .fetch_optional(&pool)
            .await;

            match row {
                Ok(Some(row)) => {
                    use sqlx::Row;
                    let url: String = row.get("url");
                    let secret: String = row.get("secret");
                    let payload: serde_json::Value = row.get("payload");
                    let payload_str = payload.to_string();
                    let signature = sign_payload(&payload_str, &secret);
                    deliver_webhook(event_id, &url, &payload_str, &signature, &pool).await;
                }
                Ok(None) => {
                    tracing::warn!(event_id = %event_id, "Webhook event not found for delivery");
                }
                Err(e) => {
                    tracing::error!(error = %e, event_id = %event_id, "Failed to fetch webhook event for delivery");
                }
            }
        }
    });
}

// endregion: --- Delivery (fire-and-forget)

// region:    --- Internal delivery + retry

async fn deliver_webhook(event_id: Uuid, url: &str, payload: &str, signature: &str, pool: &PgPool) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let timestamp = chrono::Utc::now().timestamp();

    let result = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Webhook-Signature", signature)
        .header("X-Webhook-Timestamp", timestamp.to_string())
        .header("X-Webhook-Id", event_id.to_string())
        .body(payload.to_string())
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            if let Err(e) = sqlx::query(
                "UPDATE webhook_events SET delivered_at = NOW(), attempts = attempts + 1
                 WHERE id = $1",
            )
            .bind(event_id)
            .execute(pool)
            .await
            {
                tracing::error!(error = %e, event_id = %event_id, "Failed to mark webhook delivered");
            }
            tracing::info!(event_id = %event_id, "Webhook delivered successfully");
        }
        Ok(resp) => {
            tracing::warn!(
                event_id = %event_id,
                status = %resp.status(),
                "Webhook delivery failed, scheduling retry"
            );
            schedule_retry(event_id, pool).await;
        }
        Err(e) => {
            tracing::warn!(
                event_id = %event_id,
                error = %e,
                "Webhook delivery error, scheduling retry"
            );
            schedule_retry(event_id, pool).await;
        }
    }
}

async fn schedule_retry(event_id: Uuid, pool: &PgPool) {
    // Exponential backoff intervals: 1min, 5min, 30min, 2hr, 24hr (5 max attempts)
    // Uses CASE to map attempt number to specific intervals
    if let Err(e) = sqlx::query(
        "UPDATE webhook_events
         SET attempts = attempts + 1,
             next_retry_at = NOW() + CASE attempts
                 WHEN 0 THEN INTERVAL '1 minute'
                 WHEN 1 THEN INTERVAL '5 minutes'
                 WHEN 2 THEN INTERVAL '30 minutes'
                 WHEN 3 THEN INTERVAL '2 hours'
                 ELSE INTERVAL '24 hours'
             END
         WHERE id = $1 AND attempts < 5",
    )
    .bind(event_id)
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, event_id = %event_id, "Failed to schedule webhook retry");
    }
}

fn sign_payload(payload: &str, secret: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Public entry point for webhook retry worker
pub async fn deliver_webhook_public(
    event_id: Uuid,
    url: &str,
    payload: &str,
    signature: &str,
    pool: &PgPool,
) {
    deliver_webhook(event_id, url, payload, signature, pool).await;
}

/// Public entry point for webhook retry worker
pub fn sign_payload_public(payload: &str, secret: &str) -> String {
    sign_payload(payload, secret)
}
