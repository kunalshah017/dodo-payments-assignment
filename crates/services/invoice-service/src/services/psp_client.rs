use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Shared HTTP client with connection pooling (reused across all PSP calls)
static PSP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to build PSP HTTP client")
});

#[derive(Debug, Serialize)]
struct PspChargeRequest {
    token: String,
    amount_cents: i64,
    currency: String,
}

#[derive(Debug, Deserialize)]
pub struct PspChargeResponse {
    pub status: String,
    pub psp_ref: Option<String>,
    pub code: Option<String>,
}

/// Call the PSP to charge a card. Times out after 5 seconds to avoid
/// hanging on tok_timeout (30s sleep).
///
/// Returns:
/// - Ok(response) for any 2xx response (PSP handled the request)
/// - Err(Timeout) for timeouts (PSP took too long)
/// - Err(ServerError) for 5xx (PSP is down/broken — retryable)
/// - Err(ClientError) for 4xx (our request was invalid — terminal failure)
pub async fn charge(
    psp_base_url: &str,
    token: &str,
    amount_cents: i64,
) -> Result<PspChargeResponse, PspError> {
    let response = PSP_CLIENT
        .post(format!("{}/charge", psp_base_url))
        .json(&PspChargeRequest {
            token: token.to_string(),
            amount_cents,
            currency: "USD".to_string(),
        })
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                PspError::Timeout
            } else {
                PspError::NetworkError(e.to_string())
            }
        })?;

    let status = response.status();

    if status.is_success() {
        let body = response
            .json::<PspChargeResponse>()
            .await
            .map_err(|e| PspError::NetworkError(e.to_string()))?;
        return Ok(body);
    }

    // Try to parse PSP error body for structured failure info
    if status.is_client_error() {
        // 4xx — PSP rejected the request (invalid token, bad request)
        // Attempt to parse the response body for failure details
        if let Ok(body) = response.json::<PspChargeResponse>().await {
            return Ok(body); // Return as a "handled" response with failed status
        }
        return Err(PspError::ClientError(status.as_u16()));
    }

    // 5xx — PSP is broken
    Err(PspError::ServerError(status.as_u16()))
}

#[derive(Debug, thiserror::Error)]
pub enum PspError {
    #[error("PSP request timed out")]
    Timeout,

    #[error("PSP network error: {0}")]
    NetworkError(String),

    #[error("PSP returned server error: {0}")]
    ServerError(u16),

    #[error("PSP returned client error: {0}")]
    ClientError(u16),
}
