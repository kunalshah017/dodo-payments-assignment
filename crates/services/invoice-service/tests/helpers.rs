//! Shared test helpers for integration tests.
//!
//! Provides helper functions for creating test data and making API calls.

#![allow(dead_code)]

use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

pub const BASE_URL: &str = "http://localhost:8080";
pub const API_KEY: &str = "dodo_test_key_1234567890abcdef";

/// Create a reqwest client (no auth header set — added per-request)
pub fn api_client() -> Client {
    Client::new()
}

/// Create a customer and return the full JSON response
pub async fn create_customer_full(client: &Client) -> Value {
    let resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": format!("Test Customer {}", Uuid::new_v4()),
            "email": format!("{}@test.com", Uuid::new_v4())
        }))
        .send()
        .await
        .expect("Failed to create customer");

    assert_eq!(resp.status(), 201, "Customer creation should return 201");
    resp.json().await.unwrap()
}

/// Create a customer and return the ID string
pub async fn create_customer(client: &Client) -> String {
    let body = create_customer_full(client).await;
    body["id"].as_str().unwrap().to_string()
}

/// Create a draft invoice for the given customer and return full response
pub async fn create_invoice_full(client: &Client, customer_id: &str) -> Value {
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-01-01",
            "line_items": [
                {"description": "Test item", "quantity": 1, "unit_amount_cents": 5000}
            ]
        }))
        .send()
        .await
        .expect("Failed to create invoice");

    assert_eq!(resp.status(), 201, "Invoice creation should return 201");
    resp.json().await.unwrap()
}

/// Create a draft invoice and return the ID string
pub async fn create_invoice(client: &Client, customer_id: &str) -> String {
    let body = create_invoice_full(client, customer_id).await;
    body["id"].as_str().unwrap().to_string()
}

/// Finalize an invoice (draft -> open)
pub async fn finalize_invoice(client: &Client, invoice_id: &str) -> Value {
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/finalize"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .expect("Failed to finalize invoice");

    assert_eq!(resp.status(), 200, "Finalize should return 200");
    resp.json().await.unwrap()
}

/// Create a finalized (open) invoice ready for payment
pub async fn create_open_invoice(client: &Client) -> String {
    let customer_id = create_customer(client).await;
    let invoice_id = create_invoice(client, &customer_id).await;
    finalize_invoice(client, &invoice_id).await;
    invoice_id
}

/// Pay an invoice and return the response (status code + body)
pub async fn pay_invoice(
    client: &Client,
    invoice_id: &str,
    card_token: &str,
    idempotency_key: &str,
) -> (u16, Value) {
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", idempotency_key)
        .json(&json!({"card_token": card_token}))
        .send()
        .await
        .expect("Failed to pay invoice");

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or_default();
    (status, body)
}
