use sqlx::PgPool;
use std::time::Duration;

use lib_core::model::webhook::WebhookEvent;

use super::webhook_dispatcher;

// region:    --- Webhook Retry Worker

/// Starts a background task that polls for webhook events due for retry
/// and re-delivers them with exponential backoff (5 max attempts).
pub fn start_worker(pool: PgPool) {
    tokio::spawn(async move {
        tracing::info!("Webhook retry worker started");
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;

            if let Err(e) = process_due_retries(&pool).await {
                tracing::error!(error = %e, "Webhook retry worker error");
            }
        }
    });
}

async fn process_due_retries(pool: &PgPool) -> anyhow::Result<()> {
    // Fetch events that are due for retry and haven't been delivered
    let events = sqlx::query_as::<_, WebhookEvent>(
        "SELECT * FROM webhook_events
         WHERE delivered_at IS NULL
           AND next_retry_at IS NOT NULL
           AND next_retry_at <= NOW()
           AND attempts < 5
         ORDER BY next_retry_at ASC
         LIMIT 50",
    )
    .fetch_all(pool)
    .await?;

    if !events.is_empty() {
        tracing::info!(count = events.len(), "Processing due webhook retries");
    }

    for event in events {
        let endpoint = sqlx::query_as::<_, lib_core::model::webhook::WebhookEndpoint>(
            "SELECT * FROM webhook_endpoints WHERE id = $1",
        )
        .bind(event.endpoint_id)
        .fetch_optional(pool)
        .await?;

        let Some(endpoint) = endpoint else {
            // Endpoint was deleted; mark event as exhausted
            sqlx::query(
                "UPDATE webhook_events SET next_retry_at = NULL WHERE id = $1",
            )
            .bind(event.id)
            .execute(pool)
            .await?;
            continue;
        };

        let payload_str = event.payload.to_string();
        let signature = webhook_dispatcher::sign_payload_public(&payload_str, &endpoint.secret);

        webhook_dispatcher::deliver_webhook_public(event.id, &endpoint.url, &payload_str, &signature, pool).await;
    }

    Ok(())
}

// endregion: --- Webhook Retry Worker
