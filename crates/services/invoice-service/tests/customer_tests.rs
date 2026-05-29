//! Integration tests for Customer endpoints.
//!
//! Covers: POST /v1/customers, GET /v1/customers, GET /v1/customers/{id}
//! Edge cases: validation, auth, not-found, duplicate emails

mod helpers;

use helpers::*;
use serde_json::{json, Value};
use uuid::Uuid;

// region:    --- Customer Creation (POST /v1/customers)

#[tokio::test]
async fn test_create_customer_success() {
    let client = api_client();
    let body = create_customer_full(&client).await;

    assert!(body["id"].is_string());
    assert!(body["name"].is_string());
    assert!(body["email"].is_string());
    assert!(body["created_at"].is_string());
}

#[tokio::test]
async fn test_create_customer_empty_name_rejected() {
    let client = api_client();
    let resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": "",
            "email": "valid@test.com"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("name"));
}

#[tokio::test]
async fn test_create_customer_whitespace_name_rejected() {
    let client = api_client();
    let resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": "   ",
            "email": "valid@test.com"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_customer_empty_email_rejected() {
    let client = api_client();
    let resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": "Valid Name",
            "email": ""
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_customer_invalid_email_format() {
    let client = api_client();
    let resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": "Valid Name",
            "email": "not-an-email"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("email"));
}

#[tokio::test]
async fn test_create_customer_duplicate_email_conflict() {
    let client = api_client();
    let unique_email = format!("dup-{}@test.com", Uuid::new_v4());

    // First creation succeeds
    let resp1 = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": "First Customer",
            "email": unique_email
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 201);

    // Second creation with same email should fail with 409
    let resp2 = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": "Second Customer",
            "email": unique_email
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), 409);
}

#[tokio::test]
async fn test_create_customer_missing_fields() {
    let client = api_client();

    // Missing email
    let resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Content-Type", "application/json")
        .body(r#"{"name": "Test"}"#)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_client_error());

    // Empty body
    let resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_client_error());
}

// endregion: --- Customer Creation

// region:    --- Customer Retrieval (GET /v1/customers/{id})

#[tokio::test]
async fn test_get_customer_success() {
    let client = api_client();
    let created = create_customer_full(&client).await;
    let id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!("{BASE_URL}/v1/customers/{id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["id"].as_str().unwrap(), id);
    assert_eq!(body["name"], created["name"]);
    assert_eq!(body["email"], created["email"]);
}

#[tokio::test]
async fn test_get_customer_not_found() {
    let client = api_client();
    let fake_id = Uuid::new_v4();

    let resp = client
        .get(format!("{BASE_URL}/v1/customers/{fake_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_get_customer_invalid_uuid() {
    let client = api_client();

    let resp = client
        .get(format!("{BASE_URL}/v1/customers/not-a-uuid"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_client_error());
}

// endregion: --- Customer Retrieval

// region:    --- Customer List (GET /v1/customers)

#[tokio::test]
async fn test_list_customers_returns_array() {
    let client = api_client();

    let resp = client
        .get(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn test_list_customers_includes_newly_created() {
    let client = api_client();
    let created = create_customer_full(&client).await;
    let created_id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!("{BASE_URL}/v1/customers"))
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
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&created_id),
        "Newly created customer should appear in list"
    );
}

// endregion: --- Customer List

// region:    --- Authentication

#[tokio::test]
async fn test_customer_endpoints_require_auth() {
    let client = api_client();

    // No auth header
    let resp = client
        .get(format!("{BASE_URL}/v1/customers"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Invalid token
    let resp = client
        .get(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", "Bearer invalid_key_12345")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Empty bearer
    let resp = client
        .get(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", "Bearer ")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// endregion: --- Authentication
