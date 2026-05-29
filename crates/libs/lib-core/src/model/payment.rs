use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

// region:    --- Payment Model

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "payment_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PaymentAttempt {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub idempotency_key: String,
    pub status: PaymentStatus,
    pub amount_cents: i64,
    pub card_token: String,
    pub psp_ref: Option<String>,
    pub failure_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// endregion: --- Payment Model

// region:    --- Payment DTOs

#[derive(Debug, Deserialize, ToSchema)]
pub struct PayInvoiceRequest {
    pub card_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaymentAttemptResponse {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub status: PaymentStatus,
    pub amount_cents: i64,
    pub psp_ref: Option<String>,
    pub failure_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<PaymentAttempt> for PaymentAttemptResponse {
    fn from(pa: PaymentAttempt) -> Self {
        Self {
            id: pa.id,
            invoice_id: pa.invoice_id,
            status: pa.status,
            amount_cents: pa.amount_cents,
            psp_ref: pa.psp_ref,
            failure_code: pa.failure_code,
            created_at: pa.created_at,
        }
    }
}

// endregion: --- Payment DTOs
