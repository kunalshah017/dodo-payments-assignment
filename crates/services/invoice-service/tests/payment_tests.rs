//! Integration tests for Payment endpoint.
//!
//! Covers: POST /v1/invoices/{id}/pay
//! Edge cases: concurrency, idempotency, PSP failures, validation, state guards

mod helpers;

use helpers::*;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

// region:    --- Successful Payment

#[tokio::test]
async fn test_successful_payment_marks_invoice_paid() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let key = format!("success-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;

    assert_eq!(status, 200);
    assert_eq!(body["status"].as_str().unwrap(), "succeeded");
    assert!(body["psp_ref"].is_string());
    assert!(body["id"].is_string());

    // Verify invoice is now paid
    let resp = client
        .get(format!("{BASE_URL}/v1/invoices/{invoice_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    let inv: Value = resp.json().await.unwrap();
    assert_eq!(inv["status"].as_str().unwrap(), "paid");
}

#[tokio::test]
async fn test_declined_payment_does_not_change_invoice_status() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let key = format!("decline-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_card_declined", &key).await;

    assert_eq!(status, 200);
    assert_eq!(body["status"].as_str().unwrap(), "failed");

    // Invoice should still be open (not corrupted)
    let resp = client
        .get(format!("{BASE_URL}/v1/invoices/{invoice_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    let inv: Value = resp.json().await.unwrap();
    assert_eq!(inv["status"].as_str().unwrap(), "open");
}

// endregion: --- Successful Payment

// region:    --- Concurrency

#[tokio::test]
async fn test_concurrent_payments_no_double_charge() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let n = 10;
    let client = Arc::new(client);
    let invoice_id = Arc::new(invoice_id);

    let mut handles = Vec::new();
    for i in 0..n {
        let client = Arc::clone(&client);
        let invoice_id = Arc::clone(&invoice_id);
        handles.push(tokio::spawn(async move {
            let resp = client
                .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
                .header("Authorization", format!("Bearer {API_KEY}"))
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", format!("conc-{invoice_id}-{i}"))
                .json(&json!({"card_token": "tok_success"}))
                .send()
                .await
                .expect("Request failed");
            let status = resp.status().as_u16();
            let body: Value = resp.json().await.unwrap_or_default();
            (status, body)
        }));
    }

    let mut success_count = 0;
    for handle in handles {
        let (status, body) = handle.await.unwrap();
        if status == 200 && body["status"].as_str() == Some("succeeded") {
            success_count += 1;
        }
    }

    assert!(
        success_count <= 1,
        "Expected at most 1 succeeded, got {success_count}"
    );
}

// endregion: --- Concurrency

// region:    --- Idempotency

#[tokio::test]
async fn test_idempotency_same_key_same_response() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;
    let key = format!("idem-{}", Uuid::new_v4());

    let (status1, body1) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status1, 200);
    let id1 = body1["id"].as_str().unwrap().to_string();

    // Retry with same key + same body
    let (status2, body2) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status2, 200);
    assert_eq!(body2["id"].as_str().unwrap(), id1);
}

#[tokio::test]
async fn test_idempotency_different_body_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;
    let key = format!("idem-diff-{}", Uuid::new_v4());

    let (status1, _) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status1, 200);

    // Different card_token with same key → 409
    let (status2, _) = pay_invoice(&client, &invoice_id, "tok_card_declined", &key).await;
    assert_eq!(status2, 409);
}

// endregion: --- Idempotency

// region:    --- PSP Failure Modes

#[tokio::test]
async fn test_psp_timeout_returns_pending() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let start = std::time::Instant::now();
    let key = format!("timeout-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_timeout", &key).await;
    let elapsed = start.elapsed();

    assert_eq!(status, 202);
    assert_eq!(body["status"].as_str().unwrap(), "pending");
    assert!(elapsed.as_secs() < 10, "Should timeout within ~5s");

    // Invoice remains open
    let resp = client
        .get(format!("{BASE_URL}/v1/invoices/{invoice_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    let inv: Value = resp.json().await.unwrap();
    assert_eq!(inv["status"].as_str().unwrap(), "open");
}

#[tokio::test]
async fn test_psp_network_error_returns_pending() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let key = format!("neterr-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_network_error", &key).await;

    assert_eq!(status, 202);
    assert_eq!(body["status"].as_str().unwrap(), "pending");
}

// endregion: --- PSP Failure Modes

// region:    --- State Guards

#[tokio::test]
async fn test_pay_already_paid_invoice_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let key1 = format!("first-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &invoice_id, "tok_success", &key1).await;
    assert_eq!(status, 200);

    // Second payment attempt with different key
    let key2 = format!("second-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &invoice_id, "tok_success", &key2).await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn test_pay_draft_invoice_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;

    let key = format!("draft-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn test_pay_voided_invoice_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    // Void it
    client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    let key = format!("void-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &invoice_id, "tok_success", &key).await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn test_pay_nonexistent_invoice_returns_404() {
    let client = api_client();
    let fake_id = Uuid::new_v4();

    let key = format!("fake-{}", Uuid::new_v4());
    let (status, _) = pay_invoice(&client, &fake_id.to_string(), "tok_success", &key).await;
    assert_eq!(status, 404);
}

// endregion: --- State Guards

// region:    --- Input Validation

#[tokio::test]
async fn test_pay_missing_idempotency_key_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({"card_token": "tok_success"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_pay_empty_idempotency_key_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", "")
        .json(&json!({"card_token": "tok_success"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_pay_empty_card_token_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", format!("empty-tok-{}", Uuid::new_v4()))
        .json(&json!({"card_token": ""}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_pay_whitespace_card_token_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", format!("ws-tok-{}", Uuid::new_v4()))
        .json(&json!({"card_token": "   "}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_pay_oversized_idempotency_key_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let oversized_key = "x".repeat(257);
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", oversized_key)
        .json(&json!({"card_token": "tok_success"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_pay_no_auth_returns_401() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/pay"))
        .header("Idempotency-Key", "test-key")
        .json(&json!({"card_token": "tok_success"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

// endregion: --- Input Validation

// region:    --- After Failed Payment, Can Retry

#[tokio::test]
async fn test_retry_after_declined_payment() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    // First attempt: declined
    let key1 = format!("declined-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_card_declined", &key1).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"].as_str().unwrap(), "failed");

    // Second attempt with new key should succeed (invoice still open)
    let key2 = format!("retry-{}", Uuid::new_v4());
    let (status, body) = pay_invoice(&client, &invoice_id, "tok_success", &key2).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"].as_str().unwrap(), "succeeded");

    // Invoice now paid
    let resp = client
        .get(format!("{BASE_URL}/v1/invoices/{invoice_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    let inv: Value = resp.json().await.unwrap();
    assert_eq!(inv["status"].as_str().unwrap(), "paid");
}

// endregion: --- After Failed Payment, Can Retry
