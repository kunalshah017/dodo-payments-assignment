# API Reference

**Base URL**: `http://localhost:8080`  
**Swagger UI**: `http://localhost:8080/swagger-ui/`  
**OpenAPI JSON**: `http://localhost:8080/api-docs/openapi.json`

## Authentication

All endpoints (except `/health`) require a Bearer token:

```
Authorization: Bearer <api_key>
```

API keys are prefixed with `dodo_` and scoped to a single business.

## Error Format

All errors return a consistent JSON structure:

```json
{
  "error": {
    "type": "not_found",
    "message": "Invoice 550e8400-... not found"
  }
}
```

| HTTP Status | Error Type             | Meaning                        |
| ----------- | ---------------------- | ------------------------------ |
| 400         | `bad_request`          | Invalid input / missing fields |
| 401         | `unauthorized`         | Missing or invalid API key     |
| 404         | `not_found`            | Resource doesn't exist         |
| 409         | `conflict`             | State conflict or idempotency  |
| 422         | `unprocessable_entity` | Semantically invalid request   |
| 500         | `internal_error`       | Server error (details logged)  |

---

## Health Check

### `GET /health`

No authentication required.

**Response**: `200 OK` — `"ok"`

---

## Customers

### `POST /v1/customers`

Create a new customer.

**Request Body**:

```json
{
  "name": "Acme Corp",
  "email": "billing@acme.com"
}
```

**Response**: `201 Created`

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "Acme Corp",
  "email": "billing@acme.com",
  "created_at": "2024-01-15T10:30:00Z"
}
```

### `GET /v1/customers`

List all customers for the authenticated business.

**Response**: `200 OK` — Array of customer objects.

### `GET /v1/customers/{id}`

Get a specific customer by UUID.

**Response**: `200 OK` — Single customer object.  
**Error**: `404` if not found or belongs to another business.

---

## Invoices

### `POST /v1/invoices`

Create a new invoice in `draft` status.

**Request Body**:

```json
{
  "customer_id": "550e8400-e29b-41d4-a716-446655440000",
  "due_date": "2024-02-15",
  "line_items": [
    {
      "description": "Consulting (Jan 2024)",
      "quantity": 10,
      "unit_amount_cents": 15000
    },
    {
      "description": "Hosting fees",
      "quantity": 1,
      "unit_amount_cents": 5000
    }
  ]
}
```

**Response**: `201 Created`

```json
{
  "id": "660e8400-e29b-41d4-a716-446655440000",
  "customer_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "draft",
  "total_amount_cents": 155000,
  "due_date": "2024-02-15",
  "line_items": [
    {
      "id": "770e8400-...",
      "description": "Consulting (Jan 2024)",
      "quantity": 10,
      "unit_amount_cents": 15000,
      "total_cents": 150000
    },
    {
      "id": "880e8400-...",
      "description": "Hosting fees",
      "quantity": 1,
      "unit_amount_cents": 5000,
      "total_cents": 5000
    }
  ],
  "created_at": "2024-01-15T10:30:00Z"
}
```

**Validation**:

- At least one line item required
- `quantity` must be > 0
- `unit_amount_cents` must be >= 0
- `description` must be non-empty
- `customer_id` must belong to the authenticated business

### `GET /v1/invoices`

List invoices. Optional filter by status.

**Query Parameters**:
| Param | Type | Description |
| -------- | ------ | -------------------------------------------- |
| `status` | string | Filter: `draft`, `open`, `paid`, `void`, `uncollectible` |

**Example**: `GET /invoices?status=open`

**Response**: `200 OK` — Array of invoice objects.

### `GET /v1/invoices/{id}`

Get a specific invoice with its line items.

**Response**: `200 OK` — Single invoice object.

### `POST /v1/invoices/{id}/finalize`

Transition an invoice from `draft` → `open`.

**Response**: `200 OK` — Updated invoice.  
**Error**: `409` if invoice is not in `draft` state.

### `POST /v1/invoices/{id}/void`

Void an invoice. Works from `draft` or `open` states.

**Response**: `200 OK` — Updated invoice with `status: "void"`.  
**Error**: `409` if invoice is in a terminal state (`paid`, `void`, `uncollectible`).

### `POST /v1/invoices/{id}/mark-uncollectible`

Mark an open invoice as uncollectible (e.g., after exhausting collection attempts).

**Response**: `200 OK` — Updated invoice with `status: "uncollectible"`.  
**Error**: `409` if invoice is not in `open` state.

---

## Payments

### `POST /v1/invoices/{id}/pay`

Attempt to pay an open invoice via the PSP.

**Required Headers**:
| Header | Description |
| ----------------- | ------------------------------------------------ |
| `Idempotency-Key` | Unique key per payment attempt (required) |

**Request Body**:

```json
{
  "card_token": "tok_success"
}
```

**Test Card Tokens** (mock PSP):
| Token | Behavior |
| ------------------------ | --------------------------------- |
| `tok_success` | Payment succeeds |
| `tok_insufficient_funds` | Fails with `insufficient_funds` |
| `tok_card_declined` | Fails with `card_declined` |
| `tok_timeout` | PSP hangs 30s (we timeout at 5s) |
| `tok_network_error` | PSP returns 500 |

**Responses**:

`200 OK` — Payment succeeded:

```json
{
  "id": "990e8400-...",
  "invoice_id": "660e8400-...",
  "status": "succeeded",
  "amount_cents": 155000,
  "psp_ref": "psp_ch_abc123",
  "failure_code": null,
  "created_at": "2024-01-15T10:35:00Z"
}
```

`200 OK` — Payment failed (card declined):

```json
{
  "id": "990e8400-...",
  "invoice_id": "660e8400-...",
  "status": "failed",
  "amount_cents": 155000,
  "psp_ref": null,
  "failure_code": "card_declined",
  "created_at": "2024-01-15T10:35:00Z"
}
```

`202 Accepted` — PSP timeout (payment stays pending):

```json
{
  "id": "990e8400-...",
  "invoice_id": "660e8400-...",
  "status": "pending",
  "amount_cents": 155000,
  "psp_ref": null,
  "failure_code": null,
  "created_at": "2024-01-15T10:35:00Z"
}
```

**Idempotency**: Repeating the same `Idempotency-Key` with the same `card_token` returns the original response. Different `card_token` with the same key returns `409 Conflict`.

**Concurrency**: Concurrent payments to the same invoice are serialized via row-level locking. Only one can succeed.

---

## Webhooks

### `POST /v1/webhooks/endpoints`

Register a webhook endpoint for the authenticated business.

**Request Body**:

```json
{
  "url": "https://example.com/webhooks"
}
```

**Response**: `201 Created`

```json
{
  "id": "aa0e8400-...",
  "url": "https://example.com/webhooks",
  "secret": "whsec_a1b2c3d4...",
  "created_at": "2024-01-15T10:30:00Z"
}
```

The `secret` is used to verify webhook signatures. Store it securely.

### `GET /v1/webhooks/endpoints`

List all webhook endpoints for the authenticated business.

**Response**: `200 OK` — Array of endpoint objects.

### Webhook Delivery

When events occur, the service delivers webhooks to all registered endpoints:

**Event Types**: `invoice_created`, `invoice_paid`, `invoice_payment_failed`

**Delivery Headers**:
| Header | Description |
| ---------------------- | ------------------------------------ |
| `X-Webhook-Id` | Unique event UUID |
| `X-Webhook-Timestamp` | Unix timestamp of delivery |
| `X-Webhook-Signature` | HMAC-SHA256 hex digest of body |

**Signature Verification** (pseudocode):

```
expected = HMAC-SHA256(endpoint.secret, raw_body)
valid = constant_time_compare(header["X-Webhook-Signature"], hex(expected))
```

**Retry Policy**: 5 attempts with escalating backoff (1 min, 5 min, 30 min, 2 hours, 24 hours).
