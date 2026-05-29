use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

// region:    --- Invoice Status

/// Invoice states: draft -> open -> paid | void | uncollectible
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "invoice_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Open,
    Paid,
    Void,
    Uncollectible,
}

impl InvoiceStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Paid | Self::Void | Self::Uncollectible)
    }

    pub fn valid_transitions(&self) -> &[InvoiceStatus] {
        match self {
            Self::Draft => &[Self::Open, Self::Void],
            Self::Open => &[Self::Paid, Self::Void, Self::Uncollectible],
            Self::Paid => &[],
            Self::Void => &[],
            Self::Uncollectible => &[],
        }
    }

    pub fn can_transition_to(&self, target: &InvoiceStatus) -> bool {
        self.valid_transitions().contains(target)
    }
}

// endregion: --- Invoice Status

// region:    --- Invoice Model

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub business_id: Uuid,
    pub customer_id: Uuid,
    pub status: InvoiceStatus,
    pub total_amount_cents: i64,
    pub due_date: NaiveDate,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LineItem {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
    pub total_cents: i64,
}

// endregion: --- Invoice Model

// region:    --- Invoice DTOs

#[derive(Debug, Deserialize, ToSchema)]
pub struct InvoiceCreate {
    pub customer_id: Uuid,
    pub due_date: NaiveDate,
    pub line_items: Vec<LineItemCreate>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LineItemCreate {
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvoiceResponse {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub status: InvoiceStatus,
    pub total_amount_cents: i64,
    pub due_date: NaiveDate,
    pub line_items: Vec<LineItemResponse>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LineItemResponse {
    pub id: Uuid,
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
    pub total_cents: i64,
}

impl From<LineItem> for LineItemResponse {
    fn from(li: LineItem) -> Self {
        Self {
            id: li.id,
            description: li.description,
            quantity: li.quantity,
            unit_amount_cents: li.unit_amount_cents,
            total_cents: li.total_cents,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InvoiceListQuery {
    pub status: Option<InvoiceStatus>,
}

// endregion: --- Invoice DTOs
