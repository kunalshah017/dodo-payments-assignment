pub mod customers;
pub mod invoices;
pub mod payments;
pub mod webhooks;

use axum::{
    middleware as axum_middleware,
    routing::{get, post},
    Router,
};
use lib_auth::middleware::mw_auth;

use super::AppState;

/// Creates the v1 API router with all authenticated endpoints.
pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        // Customers
        .route("/customers", post(customers::create_customer))
        .route("/customers", get(customers::list_customers))
        .route("/customers/{id}", get(customers::get_customer))
        // Invoices
        .route("/invoices", post(invoices::create_invoice))
        .route("/invoices", get(invoices::list_invoices))
        .route("/invoices/{id}", get(invoices::get_invoice))
        .route("/invoices/{id}/finalize", post(invoices::finalize_invoice))
        .route("/invoices/{id}/void", post(invoices::void_invoice))
        .route(
            "/invoices/{id}/mark-uncollectible",
            post(invoices::mark_uncollectible),
        )
        // Payments
        .route("/invoices/{id}/pay", post(payments::pay_invoice))
        // Webhooks
        .route("/webhooks/endpoints", post(webhooks::create_endpoint))
        .route("/webhooks/endpoints", get(webhooks::list_endpoints))
        .layer(axum_middleware::from_fn_with_state(
            state.db.clone(),
            mw_auth,
        ))
}
