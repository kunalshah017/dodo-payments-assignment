use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use sqlx::PgPool;

use lib_core::ctx::Ctx;
use lib_core::model::business::Business;
use lib_core::{Error, Result};

use crate::token::hash_api_key;

// region:    --- Auth Middleware

/// Axum middleware that authenticates requests via Bearer token.
/// On success, inserts `Ctx` (business context) into request extensions.
pub async fn mw_auth(State(pool): State<PgPool>, mut req: Request, next: Next) -> Result<Response> {
    let api_key = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(Error::Unauthorized)?;

    let key_hash = hash_api_key(api_key);

    let business = sqlx::query_as::<_, Business>(
        "SELECT b.id, b.name, b.created_at, b.updated_at
         FROM businesses b
         INNER JOIN api_keys ak ON ak.business_id = b.id
         WHERE ak.key_hash = $1 AND ak.revoked_at IS NULL",
    )
    .bind(&key_hash)
    .fetch_optional(&pool)
    .await
    .map_err(|e| Error::Internal(e.into()))?
    .ok_or(Error::Unauthorized)?;

    let ctx = Ctx::new(business.id);
    req.extensions_mut().insert(ctx);
    req.extensions_mut().insert(business);

    Ok(next.run(req).await)
}

// endregion: --- Auth Middleware
