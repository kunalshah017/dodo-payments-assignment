// region:    --- Modules
mod routes;
mod services;
// endregion: --- Modules

use lib_core::config::config;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use lib_core::error::{ErrorBody, ErrorDetail};
use lib_core::model::customer::*;
use lib_core::model::invoice::*;
use lib_core::model::payment::*;
use lib_core::model::webhook::*;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Dodo Payments - Invoice & Payment Service",
        version = "0.1.0",
        description = "Production invoice and payment processing API",
    ),
    paths(
        routes::v1::customers::create_customer,
        routes::v1::customers::get_customer,
        routes::v1::customers::list_customers,
        routes::v1::invoices::create_invoice,
        routes::v1::invoices::get_invoice,
        routes::v1::invoices::list_invoices,
        routes::v1::invoices::finalize_invoice,
        routes::v1::invoices::void_invoice,
        routes::v1::invoices::mark_uncollectible,
        routes::v1::payments::pay_invoice,
        routes::v1::webhooks::create_endpoint,
        routes::v1::webhooks::list_endpoints,
    ),
    components(schemas(
        CustomerCreate, CustomerResponse,
        InvoiceCreate, LineItemCreate, InvoiceResponse, LineItemResponse, InvoiceStatus, InvoiceListQuery,
        PayInvoiceRequest, PaymentAttemptResponse, PaymentStatus,
        WebhookEndpointCreate, WebhookEndpointResponse, WebhookEventType,
        ErrorBody, ErrorDetail,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "Customers", description = "Customer management"),
        (name = "Invoices", description = "Invoice lifecycle management"),
        (name = "Payments", description = "Payment processing"),
        (name = "Webhooks", description = "Webhook endpoint management"),
    )
)]
struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "invoice_service=debug,tower_http=debug".into()),
        )
        .init();

    // Config is loaded once via OnceLock on first access
    let cfg = config();

    let pool = PgPoolOptions::new()
        .max_connections(cfg.DB_MAX_CONNECTIONS)
        .connect(&cfg.DATABASE_URL)
        .await?;

    tracing::info!("Running database migrations");
    sqlx::migrate!("../../../migrations").run(&pool).await?;

    // Start webhook retry worker in the background
    services::webhook_retry::start_worker(pool.clone());

    let app_state = routes::AppState {
        db: pool,
        psp_base_url: cfg.PSP_BASE_URL.clone(),
    };

    let app = routes::create_router(app_state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.PORT));
    tracing::info!("Invoice service listening on {}", addr);
    tracing::info!("Swagger UI available at http://localhost:{}/swagger-ui/", cfg.PORT);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
