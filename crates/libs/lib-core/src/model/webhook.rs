use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

// region:    --- Webhook Model

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub business_id: Uuid,
    pub url: String,
    pub secret: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "webhook_event_type")]
pub enum WebhookEventType {
    #[sqlx(rename = "invoice_created")]
    #[serde(rename = "invoice.created")]
    InvoiceCreated,
    #[sqlx(rename = "invoice_paid")]
    #[serde(rename = "invoice.paid")]
    InvoicePaid,
    #[sqlx(rename = "invoice_payment_failed")]
    #[serde(rename = "invoice.payment_failed")]
    InvoicePaymentFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WebhookEvent {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub event_type: WebhookEventType,
    pub payload: serde_json::Value,
    pub attempts: i32,
    pub delivered_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// endregion: --- Webhook Model

// region:    --- Webhook DTOs

#[derive(Debug, Deserialize, ToSchema)]
pub struct WebhookEndpointCreate {
    pub url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookEndpointResponse {
    pub id: Uuid,
    pub url: String,
    pub secret: String,
    pub created_at: DateTime<Utc>,
}

impl From<WebhookEndpoint> for WebhookEndpointResponse {
    fn from(e: WebhookEndpoint) -> Self {
        Self {
            id: e.id,
            url: e.url,
            secret: e.secret,
            created_at: e.created_at,
        }
    }
}

// endregion: --- Webhook DTOs
