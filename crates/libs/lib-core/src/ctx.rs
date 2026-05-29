use uuid::Uuid;

/// Request context — carries authenticated business identity through the request lifecycle.
/// Extracted by auth middleware and available to all route handlers.
#[derive(Debug, Clone)]
pub struct Ctx {
    business_id: Uuid,
}

impl Ctx {
    pub fn new(business_id: Uuid) -> Self {
        Self { business_id }
    }

    pub fn business_id(&self) -> Uuid {
        self.business_id
    }
}
