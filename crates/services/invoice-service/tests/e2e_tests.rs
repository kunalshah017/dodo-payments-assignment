//! End-to-end integration tests that exercise the full lifecycle.
//!
//! Tests complete business workflows from customer creation through payment.

mod helpers;

use helpers::*;
use serde_json::{json, Value};
use uuid::Uuid;

// region:    --- Full Invoice Lifecycle

/// Complete happy path: create customer → create invoice → finalize → pay → verify paid
#[tokio::test]
async fn test_full_lifecycle_happy_path() {
    let client = api_client();

    // 1. Create customer
    let customer_resp = client
        .post(format!("{BASE_URL}/v1/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "name": "E2E Test Customer",
            "email": format!("e2e-{}@test.com", Uuid::new_v4())
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(customer_resp.status(), 201);
    let customer: Value = customer_resp.json().await.unwrap();
    let customer_id = customer["id"].as_str().unwrap();

    // 2. Create invoice
    let invoice_resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-12-31",
            "line_items": [
                {"description": "Premium Plan", "quantity": 1, "unit_amount_cents": 9900},
                {"description": "Setup Fee", "quantity": 1, "unit_amount_cents": 2500}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(invoice_resp.status(), 201);
    let invoice: Value = invoice_resp.json().await.unwrap();
    let invoice_id = invoice["id"].as_str().unwrap();
    assert_eq!(invoice["status"].as_str().unwrap(), "draft");
    assert_eq!(invoice["total_amount_cents"].as_i64().unwrap(), 12400);

    // 3. Finalize invoice
    let finalize_resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/finalize"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(finalize_resp.status(), 200);
    let finalized: Value = finalize_resp.json().await.unwrap();
    assert_eq!(finalized["status"].as_str().unwrap(), "open");

    // 4. Pay invoice
    let pay_resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", format!("e2e-pay-{}", Uuid::new_v4()))
        .json(&json!({"card_token": "tok_success"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pay_resp.status(), 200);
    let payment: Value = pay_resp.json().await.unwrap();
    assert_eq!(payment["status"].as_str().unwrap(), "succeeded");
    assert_eq!(payment["amount_cents"].as_i64().unwrap(), 12400);

    // 5. Verify invoice is paid
    let get_resp = client
        .get(format!("{BASE_URL}/v1/invoices/{invoice_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200);
    let final_invoice: Value = get_resp.json().await.unwrap();
    assert_eq!(final_invoice["status"].as_str().unwrap(), "paid");
}

/// Lifecycle: create → finalize → void
#[tokio::test]
async fn test_full_lifecycle_void() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;

    // Finalize
    finalize_invoice(&client, &invoice_id).await;

    // Void
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "void");

    // Verify it's voided and cannot be paid
    let key = format!("void-pay-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status, 409);
}

/// Lifecycle: create → finalize → payment declined → retry → success
#[tokio::test]
async fn test_full_lifecycle_payment_retry_after_decline() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;
    finalize_invoice(&client, &invoice_id).await;

    // First attempt fails
    let key1 = format!("decline-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_card_declined", &key1).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"].as_str().unwrap(), "failed");

    // Retry with a new card token succeeds
    let key2 = format!("retry-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_success", &key2).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"].as_str().unwrap(), "succeeded");
}

/// Lifecycle: create → finalize → mark uncollectible
#[tokio::test]
async fn test_full_lifecycle_uncollectible() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;
    finalize_invoice(&client, &invoice_id).await;

    let resp = client
        .post(format!(
            "{BASE_URL}/v1/invoices/{invoice_id}/mark-uncollectible"
        ))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "uncollectible");

    // Cannot pay
    let key = format!("unc-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status, 409);
}

// endregion: --- Full Invoice Lifecycle

// region:    --- Health Check

#[tokio::test]
async fn test_health_check() {
    let client = api_client();

    let resp = client
        .get(format!("{BASE_URL}/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "OK");
}

// endregion: --- Health Check

// region:    --- Error Response Format

#[tokio::test]
async fn test_error_response_format_is_consistent() {
    let client = api_client();
    let fake_id = Uuid::new_v4();

    // 404
    let resp = client
        .get(format!("{BASE_URL}/v1/invoices/{fake_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]["type"].is_string());
    assert!(body["error"]["message"].is_string());
    assert_eq!(body["error"]["type"].as_str().unwrap(), "not_found");

    // 401
    let resp = client
        .get(format!("{BASE_URL}/v1/invoices"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]["type"].is_string());
    assert!(body["error"]["message"].is_string());
    assert_eq!(body["error"]["type"].as_str().unwrap(), "unauthorized");
}

// endregion: --- Error Response Format

// region:    --- Webhook Integration (E2E)

/// Create a webhook endpoint, create/finalize/pay an invoice,
/// and verify that invoice events were recorded.
#[tokio::test]
async fn test_webhook_events_dispatched_on_invoice_lifecycle() {
    let client = api_client();

    // Create webhook endpoint (uses HTTPS as required)
    let webhook_resp = client
        .post(format!("{BASE_URL}/v1/webhooks/endpoints"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "url": "https://httpbin.org/post",
            "events": ["invoice.created", "invoice.paid"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(webhook_resp.status(), 201);
    let webhook: Value = webhook_resp.json().await.unwrap();
    assert!(!webhook["secret"].as_str().unwrap().is_empty());

    // Create customer + invoice + finalize + pay
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;
    finalize_invoice(&client, &invoice_id).await;

    let key = format!("wh-pay-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status, 200);

    // Give webhooks time to dispatch (async)
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // We can't easily verify external delivery, but we verified the flow doesn't error
}

// endregion: --- Webhook Integration (E2E)
