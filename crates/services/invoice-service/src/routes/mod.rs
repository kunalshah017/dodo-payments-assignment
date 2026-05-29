pub mod v1;

use axum::{routing::get, Router};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub psp_base_url: String,
}

pub fn create_router(state: AppState) -> Router {
    let public_routes = Router::new().route("/health", get(health_check));

    Router::new()
        .merge(public_routes)
        .nest("/v1", v1::routes(&state))
        .with_state(state)
}

async fn health_check() -> &'static str {
    "OK"
}
