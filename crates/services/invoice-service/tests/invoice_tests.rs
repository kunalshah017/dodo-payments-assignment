//! Integration tests for Invoice endpoints.
//!
//! Covers: All CRUD + state transitions + validation + edge cases
//! State machine: draft → open → paid | void | uncollectible

mod helpers;

use helpers::*;
use serde_json::{json, Value};
use uuid::Uuid;

// region:    --- Invoice Creation (POST /v1/invoices)

#[tokio::test]
async fn test_create_invoice_success() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let body = create_invoice_full(&client, &customer_id).await;

    assert_eq!(body["status"].as_str().unwrap(), "draft");
    assert_eq!(body["customer_id"].as_str().unwrap(), customer_id);
    assert_eq!(body["total_amount_cents"].as_i64().unwrap(), 5000);
    assert!(body["line_items"].is_array());
    assert_eq!(body["line_items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_create_invoice_multiple_line_items() {
    let client = api_client();
    let customer_id = create_customer(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-06-15",
            "line_items": [
                {"description": "Widget A", "quantity": 2, "unit_amount_cents": 1000},
                {"description": "Widget B", "quantity": 3, "unit_amount_cents": 500},
                {"description": "Service Fee", "quantity": 1, "unit_amount_cents": 2500}
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await.unwrap();
    // Total: 2*1000 + 3*500 + 1*2500 = 2000 + 1500 + 2500 = 6000
    assert_eq!(body["total_amount_cents"].as_i64().unwrap(), 6000);
    assert_eq!(body["line_items"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_create_invoice_empty_line_items_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-01-01",
            "line_items": []
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("line item"));
}

#[tokio::test]
async fn test_create_invoice_zero_quantity_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-01-01",
            "line_items": [
                {"description": "Item", "quantity": 0, "unit_amount_cents": 5000}
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_invoice_negative_quantity_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-01-01",
            "line_items": [
                {"description": "Item", "quantity": -1, "unit_amount_cents": 5000}
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_invoice_negative_unit_amount_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-01-01",
            "line_items": [
                {"description": "Item", "quantity": 1, "unit_amount_cents": -100}
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_invoice_empty_description_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2027-01-01",
            "line_items": [
                {"description": "", "quantity": 1, "unit_amount_cents": 5000}
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_invoice_nonexistent_customer_rejected() {
    let client = api_client();
    let fake_customer = Uuid::new_v4();

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": fake_customer.to_string(),
            "due_date": "2027-01-01",
            "line_items": [
                {"description": "Item", "quantity": 1, "unit_amount_cents": 5000}
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

// endregion: --- Invoice Creation

// region:    --- Invoice Retrieval

#[tokio::test]
async fn test_get_invoice_success() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let created = create_invoice_full(&client, &customer_id).await;
    let id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!("{BASE_URL}/v1/invoices/{id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["id"].as_str().unwrap(), id);
    assert_eq!(body["status"].as_str().unwrap(), "draft");
}

#[tokio::test]
async fn test_get_invoice_not_found() {
    let client = api_client();
    let fake_id = Uuid::new_v4();

    let resp = client
        .get(format!("{BASE_URL}/v1/invoices/{fake_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_list_invoices_returns_array() {
    let client = api_client();

    let resp = client
        .get(format!("{BASE_URL}/v1/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn test_list_invoices_filter_by_status() {
    let client = api_client();
    // Create an open invoice
    let _open_id = create_open_invoice(&client).await;

    // Filter by open
    let resp = client
        .get(format!("{BASE_URL}/v1/invoices?status=open"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let arr = body.as_array().unwrap();
    for inv in arr {
        assert_eq!(inv["status"].as_str().unwrap(), "open");
    }
}

// endregion: --- Invoice Retrieval

// region:    --- State Transitions: Finalize (draft → open)

#[tokio::test]
async fn test_finalize_draft_invoice_success() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/finalize"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "open");
}

#[tokio::test]
async fn test_finalize_already_open_invoice_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/finalize"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_finalize_paid_invoice_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    // Pay it first
    let key = format!("pay-{}", Uuid::new_v4());
    pay_invoice(&client, &invoice_id, "tok_success", &key).await;

    // Now try to finalize again
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/finalize"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_finalize_nonexistent_invoice() {
    let client = api_client();
    let fake_id = Uuid::new_v4();

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{fake_id}/finalize"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

// endregion: --- State Transitions: Finalize

// region:    --- State Transitions: Void

#[tokio::test]
async fn test_void_draft_invoice_success() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "void");
}

#[tokio::test]
async fn test_void_open_invoice_success() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "void");
}

#[tokio::test]
async fn test_void_paid_invoice_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let key = format!("void-pay-{}", Uuid::new_v4());
    pay_invoice(&client, &invoice_id, "tok_success", &key).await;

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_void_already_voided_invoice_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;

    // Void it
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Try to void again
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

// endregion: --- State Transitions: Void

// region:    --- State Transitions: Mark Uncollectible

#[tokio::test]
async fn test_mark_uncollectible_open_invoice_success() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

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
}

#[tokio::test]
async fn test_mark_uncollectible_draft_rejected() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;

    let resp = client
        .post(format!(
            "{BASE_URL}/v1/invoices/{invoice_id}/mark-uncollectible"
        ))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_mark_uncollectible_paid_rejected() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    let key = format!("unc-pay-{}", Uuid::new_v4());
    pay_invoice(&client, &invoice_id, "tok_success", &key).await;

    let resp = client
        .post(format!(
            "{BASE_URL}/v1/invoices/{invoice_id}/mark-uncollectible"
        ))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
}

// endregion: --- State Transitions: Mark Uncollectible

// region:    --- Cross-State Transition Guards

#[tokio::test]
async fn test_cannot_pay_voided_invoice() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    // Void it
    let resp = client
        .post(format!("{BASE_URL}/v1/invoices/{invoice_id}/void"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Try to pay
    let (status, _) = pay_invoice(
        &client,
        &invoice_id,
        "tok_success",
        &format!("void-pay-{}", Uuid::new_v4()),
    )
    .await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn test_cannot_pay_uncollectible_invoice() {
    let client = api_client();
    let invoice_id = create_open_invoice(&client).await;

    // Mark uncollectible
    let resp = client
        .post(format!(
            "{BASE_URL}/v1/invoices/{invoice_id}/mark-uncollectible"
        ))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Try to pay
    let (status, _) = pay_invoice(
        &client,
        &invoice_id,
        "tok_success",
        &format!("unc-pay-{}", Uuid::new_v4()),
    )
    .await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn test_cannot_pay_draft_invoice() {
    let client = api_client();
    let customer_id = create_customer(&client).await;
    let invoice_id = create_invoice(&client, &customer_id).await;

    let (status, _) = pay_invoice(
        &client,
        &invoice_id,
        "tok_success",
        &format!("draft-pay-{}", Uuid::new_v4()),
    )
    .await;
    assert_eq!(status, 409);
}

// endregion: --- Cross-State Transition Guards

// region:    --- Authentication

#[tokio::test]
async fn test_invoice_endpoints_require_auth() {
    let client = api_client();

    let resp = client
        .get(format!("{BASE_URL}/v1/invoices"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = client
        .post(format!("{BASE_URL}/v1/invoices"))
        .header("Content-Type", "application/json")
        .body(r#"{"customer_id":"00000000-0000-0000-0000-000000000000","due_date":"2027-01-01","line_items":[{"description":"x","quantity":1,"unit_amount_cents":100}]}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// endregion: --- Authentication
