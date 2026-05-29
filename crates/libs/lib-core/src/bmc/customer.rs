use sqlx::PgPool;
use uuid::Uuid;

use crate::ctx::Ctx;
use crate::model::customer::{Customer, CustomerCreate};
use crate::Result;

// region:    --- CustomerBmc

pub struct CustomerBmc;

impl CustomerBmc {
    pub async fn create(pool: &PgPool, ctx: &Ctx, data: CustomerCreate) -> Result<Customer> {
        let customer = sqlx::query_as::<_, Customer>(
            "INSERT INTO customers (id, business_id, name, email, created_at, updated_at)
             VALUES ($1, $2, $3, $4, NOW(), NOW())
             RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(ctx.business_id())
        .bind(data.name.trim())
        .bind(data.email.trim())
        .fetch_one(pool)
        .await?;

        Ok(customer)
    }

    pub async fn get(pool: &PgPool, ctx: &Ctx, id: Uuid) -> Result<Customer> {
        let customer = sqlx::query_as::<_, Customer>(
            "SELECT * FROM customers WHERE id = $1 AND business_id = $2",
        )
        .bind(id)
        .bind(ctx.business_id())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| crate::Error::NotFound(format!("Customer {id} not found")))?;

        Ok(customer)
    }

    pub async fn list(pool: &PgPool, ctx: &Ctx) -> Result<Vec<Customer>> {
        let customers = sqlx::query_as::<_, Customer>(
            "SELECT * FROM customers WHERE business_id = $1 ORDER BY created_at DESC",
        )
        .bind(ctx.business_id())
        .fetch_all(pool)
        .await?;

        Ok(customers)
    }
}

// endregion: --- CustomerBmc
