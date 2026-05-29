use sqlx::PgPool;
use uuid::Uuid;

use crate::ctx::Ctx;
use crate::model::invoice::*;
use crate::{Error, Result};

// region:    --- InvoiceBmc

pub struct InvoiceBmc;

impl InvoiceBmc {
    pub async fn create(
        pool: &PgPool,
        ctx: &Ctx,
        data: InvoiceCreate,
    ) -> Result<(Invoice, Vec<LineItem>)> {
        // Compute total from line items (integer math only — no floats)
        // Use checked arithmetic to prevent overflow
        let total_amount_cents: i64 = data
            .line_items
            .iter()
            .try_fold(0i64, |acc, item| {
                let line_total = item
                    .unit_amount_cents
                    .checked_mul(item.quantity as i64)
                    .ok_or_else(|| {
                        Error::BadRequest("Line item amount overflow".to_string())
                    })?;
                acc.checked_add(line_total).ok_or_else(|| {
                    Error::BadRequest("Invoice total amount overflow".to_string())
                })
            })?;

        let invoice_id = Uuid::new_v4();
        let mut tx = pool.begin().await?;

        let invoice = sqlx::query_as::<_, Invoice>(
            "INSERT INTO invoices (id, business_id, customer_id, status, total_amount_cents, due_date, created_at, updated_at)
             VALUES ($1, $2, $3, 'draft', $4, $5, NOW(), NOW())
             RETURNING *",
        )
        .bind(invoice_id)
        .bind(ctx.business_id())
        .bind(data.customer_id)
        .bind(total_amount_cents)
        .bind(data.due_date)
        .fetch_one(&mut *tx)
        .await?;

        let mut line_items = Vec::new();
        for item in &data.line_items {
            let line_total = item
                .unit_amount_cents
                .checked_mul(item.quantity as i64)
                .ok_or_else(|| Error::BadRequest("Line item amount overflow".to_string()))?;
            let li = sqlx::query_as::<_, LineItem>(
                "INSERT INTO line_items (id, invoice_id, description, quantity, unit_amount_cents, total_cents)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING *",
            )
            .bind(Uuid::new_v4())
            .bind(invoice_id)
            .bind(item.description.trim())
            .bind(item.quantity)
            .bind(item.unit_amount_cents)
            .bind(line_total)
            .fetch_one(&mut *tx)
            .await?;

            line_items.push(li);
        }

        tx.commit().await?;
        Ok((invoice, line_items))
    }

    pub async fn get(pool: &PgPool, ctx: &Ctx, id: Uuid) -> Result<(Invoice, Vec<LineItem>)> {
        let invoice = sqlx::query_as::<_, Invoice>(
            "SELECT * FROM invoices WHERE id = $1 AND business_id = $2",
        )
        .bind(id)
        .bind(ctx.business_id())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Invoice {id} not found")))?;

        let line_items =
            sqlx::query_as::<_, LineItem>("SELECT * FROM line_items WHERE invoice_id = $1")
                .bind(id)
                .fetch_all(pool)
                .await?;

        Ok((invoice, line_items))
    }

    pub async fn list(
        pool: &PgPool,
        ctx: &Ctx,
        filter: InvoiceListQuery,
    ) -> Result<Vec<(Invoice, Vec<LineItem>)>> {
        let invoices = if let Some(status) = filter.status {
            sqlx::query_as::<_, Invoice>(
                "SELECT * FROM invoices WHERE business_id = $1 AND status = $2 ORDER BY created_at DESC",
            )
            .bind(ctx.business_id())
            .bind(status)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as::<_, Invoice>(
                "SELECT * FROM invoices WHERE business_id = $1 ORDER BY created_at DESC",
            )
            .bind(ctx.business_id())
            .fetch_all(pool)
            .await?
        };

        let mut results = Vec::new();
        for invoice in invoices {
            let line_items =
                sqlx::query_as::<_, LineItem>("SELECT * FROM line_items WHERE invoice_id = $1")
                    .bind(invoice.id)
                    .fetch_all(pool)
                    .await?;
            results.push((invoice, line_items));
        }

        Ok(results)
    }

    // region:    --- State Transitions

    /// Finalize: draft -> open
    pub async fn finalize(pool: &PgPool, ctx: &Ctx, id: Uuid) -> Result<(Invoice, Vec<LineItem>)> {
        let invoice = sqlx::query_as::<_, Invoice>(
            "SELECT * FROM invoices WHERE id = $1 AND business_id = $2",
        )
        .bind(id)
        .bind(ctx.business_id())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Invoice {id} not found")))?;

        if !invoice.status.can_transition_to(&InvoiceStatus::Open) {
            return Err(Error::Conflict(format!(
                "Cannot finalize invoice in {:?} state",
                invoice.status
            )));
        }

        let updated = sqlx::query_as::<_, Invoice>(
            "UPDATE invoices SET status = 'open', updated_at = NOW()
             WHERE id = $1 AND status = 'draft'
             RETURNING *",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            Error::Conflict("Invoice state changed concurrently".to_string())
        })?;

        let line_items =
            sqlx::query_as::<_, LineItem>("SELECT * FROM line_items WHERE invoice_id = $1")
                .bind(id)
                .fetch_all(pool)
                .await?;

        Ok((updated, line_items))
    }

    /// Void: draft|open -> void
    pub async fn void(pool: &PgPool, ctx: &Ctx, id: Uuid) -> Result<(Invoice, Vec<LineItem>)> {
        let invoice = sqlx::query_as::<_, Invoice>(
            "SELECT * FROM invoices WHERE id = $1 AND business_id = $2",
        )
        .bind(id)
        .bind(ctx.business_id())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Invoice {id} not found")))?;

        if !invoice.status.can_transition_to(&InvoiceStatus::Void) {
            return Err(Error::Conflict(format!(
                "Cannot void invoice in {:?} state",
                invoice.status
            )));
        }

        // Use parameterized status in WHERE clause (safe: value comes from our validated enum)
        let updated = sqlx::query_as::<_, Invoice>(
            "UPDATE invoices SET status = 'void', updated_at = NOW()
             WHERE id = $1 AND status = $2
             RETURNING *",
        )
        .bind(id)
        .bind(&invoice.status)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            Error::Conflict("Invoice state changed concurrently".to_string())
        })?;

        let line_items =
            sqlx::query_as::<_, LineItem>("SELECT * FROM line_items WHERE invoice_id = $1")
                .bind(id)
                .fetch_all(pool)
                .await?;

        Ok((updated, line_items))
    }

    /// Lock invoice row for payment processing (SELECT ... FOR UPDATE) within a transaction.
    /// The transaction is returned so the caller can create the payment attempt
    /// while the lock is held, preventing concurrent double-charges.
    pub async fn get_for_payment_tx(
        pool: &PgPool,
        ctx: &Ctx,
        id: Uuid,
    ) -> Result<(Invoice, sqlx::Transaction<'static, sqlx::Postgres>)> {
        let mut tx = pool.begin().await?;

        let invoice = sqlx::query_as::<_, Invoice>(
            "SELECT * FROM invoices WHERE id = $1 AND business_id = $2 FOR UPDATE",
        )
        .bind(id)
        .bind(ctx.business_id())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Invoice {id} not found")))?;

        Ok((invoice, tx))
    }

    /// Transition invoice to paid (conditional on still being 'open').
    /// Uses status-conditional UPDATE as a final safety net against races.
    /// Returns error if the invoice was concurrently transitioned to another state.
    pub async fn mark_paid(pool: &PgPool, id: Uuid) -> Result<()> {
        let result = sqlx::query(
            "UPDATE invoices SET status = 'paid', updated_at = NOW()
             WHERE id = $1 AND status = 'open'",
        )
        .bind(id)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::Conflict(
                "Invoice state changed concurrently; cannot mark as paid".to_string(),
            ));
        }

        Ok(())
    }

    /// Mark invoice as uncollectible (only from 'open' state)
    pub async fn mark_uncollectible(
        pool: &PgPool,
        ctx: &Ctx,
        id: Uuid,
    ) -> Result<(Invoice, Vec<LineItem>)> {
        let invoice = sqlx::query_as::<_, Invoice>(
            "SELECT * FROM invoices WHERE id = $1 AND business_id = $2",
        )
        .bind(id)
        .bind(ctx.business_id())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Invoice {id} not found")))?;

        if !invoice.status.can_transition_to(&InvoiceStatus::Uncollectible) {
            return Err(Error::Conflict(format!(
                "Cannot mark invoice as uncollectible in {:?} state",
                invoice.status
            )));
        }

        let updated = sqlx::query_as::<_, Invoice>(
            "UPDATE invoices SET status = 'uncollectible', updated_at = NOW()
             WHERE id = $1 AND status = 'open'
             RETURNING *",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            Error::Conflict("Invoice state changed concurrently".to_string())
        })?;

        let line_items =
            sqlx::query_as::<_, LineItem>("SELECT * FROM line_items WHERE invoice_id = $1")
                .bind(id)
                .fetch_all(pool)
                .await?;

        Ok((updated, line_items))
    }

    /// Check if customer belongs to business
    pub async fn validate_customer(pool: &PgPool, ctx: &Ctx, customer_id: Uuid) -> Result<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND business_id = $2)",
        )
        .bind(customer_id)
        .bind(ctx.business_id())
        .fetch_one(pool)
        .await?;

        Ok(exists)
    }

    // endregion: --- State Transitions
}

// endregion: --- InvoiceBmc
