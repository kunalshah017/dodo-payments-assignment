use axum::{extract::Json, http::StatusCode, routing::post, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

#[derive(Deserialize)]
struct ChargeRequest {
    token: String,
    amount_cents: i64,
}

#[derive(Serialize)]
struct ChargeResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    psp_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

async fn charge(Json(req): Json<ChargeRequest>) -> (StatusCode, Json<ChargeResponse>) {
    tracing::info!(token = %req.token, amount = req.amount_cents, "Processing charge");

    match req.token.as_str() {
        "tok_success" => {
            sleep(Duration::from_millis(100)).await;
            (
                StatusCode::OK,
                Json(ChargeResponse {
                    status: "succeeded".to_string(),
                    psp_ref: Some(Uuid::new_v4().to_string()),
                    code: None,
                }),
            )
        }
        "tok_insufficient_funds" => {
            sleep(Duration::from_millis(100)).await;
            (
                StatusCode::OK,
                Json(ChargeResponse {
                    status: "failed".to_string(),
                    psp_ref: None,
                    code: Some("insufficient_funds".to_string()),
                }),
            )
        }
        "tok_card_declined" => {
            sleep(Duration::from_millis(100)).await;
            (
                StatusCode::OK,
                Json(ChargeResponse {
                    status: "failed".to_string(),
                    psp_ref: None,
                    code: Some("card_declined".to_string()),
                }),
            )
        }
        "tok_timeout" => {
            sleep(Duration::from_secs(30)).await;
            (
                StatusCode::OK,
                Json(ChargeResponse {
                    status: "succeeded".to_string(),
                    psp_ref: Some(Uuid::new_v4().to_string()),
                    code: None,
                }),
            )
        }
        "tok_network_error" => {
            // Return 500 to simulate network failure
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChargeResponse {
                    status: "error".to_string(),
                    psp_ref: None,
                    code: Some("network_error".to_string()),
                }),
            )
        }
        _ => {
            sleep(Duration::from_millis(100)).await;
            (
                StatusCode::BAD_REQUEST,
                Json(ChargeResponse {
                    status: "failed".to_string(),
                    psp_ref: None,
                    code: Some("invalid_token".to_string()),
                }),
            )
        }
    }
}

async fn health() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mock_psp=info".into()),
        )
        .init();

    let app = Router::new()
        .route("/charge", post(charge))
        .route("/health", axum::routing::get(health));

    let addr = SocketAddr::from(([0, 0, 0, 0], 9090));
    tracing::info!("Mock PSP listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
