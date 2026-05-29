use sqlx::PgPool;
use uuid::Uuid;

use crate::model::webhook::{WebhookEndpoint, WebhookEndpointCreate};
use crate::Result;

// region:    --- WebhookBmc

pub struct WebhookBmc;

impl WebhookBmc {
    pub async fn create_endpoint(
        pool: &PgPool,
        business_id: Uuid,
        data: WebhookEndpointCreate,
        secret: &str,
    ) -> Result<WebhookEndpoint> {
        let endpoint = sqlx::query_as::<_, WebhookEndpoint>(
            "INSERT INTO webhook_endpoints (id, business_id, url, secret, created_at)
             VALUES ($1, $2, $3, $4, NOW())
             RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(business_id)
        .bind(data.url.trim())
        .bind(secret)
        .fetch_one(pool)
        .await?;

        Ok(endpoint)
    }

    pub async fn list_endpoints(pool: &PgPool, business_id: Uuid) -> Result<Vec<WebhookEndpoint>> {
        let endpoints = sqlx::query_as::<_, WebhookEndpoint>(
            "SELECT * FROM webhook_endpoints WHERE business_id = $1 ORDER BY created_at DESC",
        )
        .bind(business_id)
        .fetch_all(pool)
        .await?;

        Ok(endpoints)
    }

    pub async fn get_endpoints_for_business(
        pool: &PgPool,
        business_id: Uuid,
    ) -> Result<Vec<WebhookEndpoint>> {
        Self::list_endpoints(pool, business_id).await
    }
}

// endregion: --- WebhookBmc
