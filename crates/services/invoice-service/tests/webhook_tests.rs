//! Integration tests for Webhook endpoints.
//!
//! Covers: POST /v1/webhooks/endpoints, GET /v1/webhooks/endpoints
//! Edge cases: URL validation, SSRF protection, auth

mod helpers;

use helpers::*;
use serde_json::{json, Value};

// region:    --- Webhook Endpoint Creation

#[tokio::test]
async fn test_create_webhook_endpoint_success() {
    let client = api_client();

    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "https://example.com/webhook",
            "events": ["invoice.created", "invoice.paid"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await.unwrap();
    assert!(body["id"].is_string());
    assert!(body["url"].is_string());
    assert!(body["secret"].is_string());
    assert_eq!(body["url"].as_str().unwrap(), "https://example.com/webhook");
}

#[tokio::test]
async fn test_create_webhook_endpoint_empty_url_rejected() {
    let client = api_client();

    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "",
            "events": ["invoice.created"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_webhook_endpoint_invalid_url_rejected() {
    let client = api_client();

    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "not-a-valid-url",
            "events": ["invoice.created"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_webhook_endpoint_http_rejected() {
    let client = api_client();

    // HTTP (non-HTTPS) should be rejected
    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "http://example.com/webhook",
            "events": ["invoice.created"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("HTTPS"));
}

#[tokio::test]
async fn test_create_webhook_endpoint_localhost_rejected() {
    let client = api_client();

    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "https://localhost/webhook",
            "events": ["invoice.created"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("private"));
}

#[tokio::test]
async fn test_create_webhook_endpoint_private_ip_rejected() {
    let client = api_client();

    // 192.168.x.x
    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "https://192.168.1.1/webhook",
            "events": ["invoice.created"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // 10.x.x.x
    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "https://10.0.0.1/webhook",
            "events": ["invoice.created"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // 127.0.0.1
    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "https://127.0.0.1/webhook",
            "events": ["invoice.created"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// endregion: --- Webhook Endpoint Creation

// region:    --- Webhook Endpoint Listing

#[tokio::test]
async fn test_list_webhook_endpoints_returns_array() {
    let client = api_client();

    let resp = client
        .get(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn test_list_webhook_endpoints_includes_created() {
    let client = api_client();

    // Create endpoint
    let create_resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "https://unique-test-webhook.example.com/hook",
            "events": ["invoice.paid"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 201);
    let created: Value = create_resp.json().await.unwrap();
    let created_id = created["id"].as_str().unwrap();

    // List
    let resp = client
        .get(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&created_id));
}

// endregion: --- Webhook Endpoint Listing

// region:    --- Authentication

#[tokio::test]
async fn test_webhook_endpoints_require_auth() {
    let client = api_client();

    let resp = client
        .get(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Content-Type", "application/json")
        .body(r#"{"url":"https://example.com/hook","events":["invoice.created"]}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// endregion: --- Authentication
