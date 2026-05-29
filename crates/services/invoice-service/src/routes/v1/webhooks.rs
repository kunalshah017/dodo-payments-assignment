use axum::{extract::State, http::StatusCode, Extension, Json};
use rand::Rng;

use lib_core::bmc::webhook::WebhookBmc;
use lib_core::ctx::Ctx;
use lib_core::error::ErrorBody;
use lib_core::model::webhook::{WebhookEndpointCreate, WebhookEndpointResponse};
use lib_core::{Error, Result};

use crate::routes::AppState;

#[utoipa::path(
    post,
    path = "/v1/webhooks/endpoints",
    request_body = WebhookEndpointCreate,
    responses(
        (status = 201, description = "Webhook endpoint created", body = WebhookEndpointResponse),
        (status = 400, description = "Validation error", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Webhooks"
)]
pub async fn create_endpoint(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Json(req): Json<WebhookEndpointCreate>,
) -> Result<(StatusCode, Json<WebhookEndpointResponse>)> {
    if req.url.trim().is_empty() {
        return Err(Error::BadRequest("url is required".to_string()));
    }

    // Validate URL format and restrict to HTTPS (prevent SSRF to internal networks)
    let parsed_url = url::Url::parse(&req.url)
        .map_err(|_| Error::BadRequest("Invalid URL format".to_string()))?;

    if parsed_url.scheme() != "https" {
        return Err(Error::BadRequest(
            "Webhook URL must use HTTPS".to_string(),
        ));
    }

    // Block common private/internal network hostnames
    let host = parsed_url.host_str().unwrap_or_default();
    if host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host == "0.0.0.0"
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("172.16.")
    {
        return Err(Error::BadRequest(
            "Webhook URL must not point to private/internal networks".to_string(),
        ));
    }

    let secret = generate_webhook_secret();
    let endpoint = WebhookBmc::create_endpoint(&state.db, ctx.business_id(), req, &secret).await?;

    Ok((StatusCode::CREATED, Json(endpoint.into())))
}

#[utoipa::path(
    get,
    path = "/v1/webhooks/endpoints",
    responses(
        (status = 200, description = "List of webhook endpoints", body = Vec<WebhookEndpointResponse>),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Webhooks"
)]
pub async fn list_endpoints(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
) -> Result<Json<Vec<WebhookEndpointResponse>>> {
    let endpoints = WebhookBmc::list_endpoints(&state.db, ctx.business_id()).await?;
    let response: Vec<WebhookEndpointResponse> = endpoints.into_iter().map(Into::into).collect();
    Ok(Json(response))
}

fn generate_webhook_secret() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    format!("whsec_{}", hex::encode(bytes))
}
