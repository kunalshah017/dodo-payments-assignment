# Invoice & Payment Service

A minimal Invoice & Payment Service built in **Rust** with **Axum**, **PostgreSQL**, and a mock PSP. Businesses create invoices for their customers, customers pay invoices, and businesses receive signed webhooks for state changes.

**GitHub**: [kunalshah017/dodo-payments-assignment](https://github.com/kunalshah017/dodo-payments-assignment)

## Quick Start

```bash
docker compose up
```

That's it. This brings up:

- **PostgreSQL** (port 5432) — with auto-migrations and seed data
- **Mock PSP** (port 9090) — simulates payment processor
- **Invoice Service** (port 8080) — the main API with Swagger UI

## Test API Key

Pre-seeded for development:

```
dodo_test_key_1234567890abcdef
```

---

## Architecture

```mermaid
graph LR
    A[Business<br/>API Client] -->|HTTP| B[Invoice Service<br/>Axum :8080]
    B -->|HTTP| C[Mock PSP<br/>Axum :9090]
    B -->|SQLx async| D[(PostgreSQL<br/>:5432)]
    B -->|async POST| E[Webhook<br/>Endpoints]
```

### Crate Structure

```
crates/
  libs/
    lib-core/        → Config, Error types, domain models, BMCs (data access)
    lib-auth/        → Auth middleware, API key hashing (SHA-256)
  services/
    invoice-service/ → HTTP handlers, PSP client, webhook dispatcher
    mock-psp/        → Mock payment processor (separate binary)
```

### Request Flow (Payment Example)

```mermaid
sequenceDiagram
    participant C as Client
    participant A as Auth Middleware
    participant H as Payment Handler
    participant DB as PostgreSQL
    participant PSP as Mock PSP
    participant W as Webhook

    C->>A: POST /v1/invoices/{id}/pay
    A->>A: Hash Bearer token → lookup business
    A->>H: Inject Ctx (business_id)
    H->>DB: Check idempotency key
    H->>DB: SELECT ... FOR UPDATE (lock invoice)
    H->>DB: INSERT payment_attempt (pending)
    H->>DB: COMMIT (release lock)
    H->>PSP: POST /charge (5s timeout)
    PSP-->>H: {status: succeeded/failed}
    H->>DB: UPDATE payment_attempt & invoice
    H-->>W: tokio::spawn webhook delivery
    H-->>C: 200 OK / 202 Accepted
```

### Data Model

```mermaid
erDiagram
    businesses ||--o{ api_keys : has
    businesses ||--o{ customers : has
    businesses ||--o{ webhook_endpoints : has
    customers ||--o{ invoices : has
    invoices ||--o{ line_items : has
    invoices ||--o{ payment_attempts : has
    webhook_endpoints ||--o{ webhook_events : has
```

All money fields are `BIGINT` (integer cents). No floats anywhere.

---

## Live Demo

### 1. Create a Customer

Every invoice belongs to a customer, and every customer belongs to the authenticated business. The API key in the `Authorization` header determines which business owns the resource — there's no `business_id` in the request body.

```bash
curl -s -X POST http://localhost:8080/v1/customers \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -d '{"name": "Jane Doe", "email": "jane@example.com"}' | jq
```

### 2. Register a Webhook Endpoint

Webhooks let the business know when things happen (invoice created, paid, payment failed) without polling. The system uses a **transactional outbox pattern** — events are inserted in the same DB transaction as the state change, so no event is ever lost even if the service crashes immediately after.

Use any URL that can receive POST requests (e.g. [webhook.site](https://webhook.site)).

```bash
curl -s -X POST http://localhost:8080/v1/webhooks/endpoints \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -d '{"url": "https://webhook.site/<your-unique-id>"}' | jq
```

The response includes a `secret` — the HMAC-SHA256 signing key used to verify webhook payloads.

### 3. Create an Invoice

Invoices start in `draft` state. The server computes the total from line items — never trust a client-supplied total. All money is integer cents (`i64` / `BIGINT`), no floats anywhere.

```bash
curl -s -X POST http://localhost:8080/v1/invoices \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "<customer_id>",
    "due_date": "2025-02-01",
    "line_items": [
      {"description": "Consulting (1hr)", "quantity": 2, "unit_amount_cents": 15000},
      {"description": "Platform fee", "quantity": 1, "unit_amount_cents": 5000}
    ]
  }' | jq
```

Total = (2 × 15000) + (1 × 5000) = **35000 cents** ($350.00), computed server-side.

### 4. Finalize Invoice (draft → open)

Payments are only allowed on `open` invoices. Finalization is the explicit transition from draft → open. This two-step model lets businesses edit invoices before they become payable — same pattern Stripe uses.

```bash
curl -s -X POST http://localhost:8080/v1/invoices/<invoice_id>/finalize \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" | jq
```

### 5. Pay Invoice — Success (tok_success)

The payment handler: acquires a row-level lock (`SELECT ... FOR UPDATE`) → creates a pending payment attempt → commits (releasing the lock) → calls the PSP. The lock is held for microseconds, not the duration of the PSP call. The `Idempotency-Key` header prevents duplicate charges on retry.

```bash
curl -s -X POST http://localhost:8080/v1/invoices/<invoice_id>/pay \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-001" \
  -d '{"card_token": "tok_success"}' | jq
```

Returns 200 with `status: "succeeded"`. Invoice is now `paid`.

### 6. Verify Invoice is Paid

After a successful payment, the invoice transitions to `paid` (a terminal state — no further transitions allowed). The webhook for `invoice.paid` was already dispatched asynchronously.

```bash
curl -s http://localhost:8080/v1/invoices/<invoice_id> \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" | jq .status
```

### 7. Pay Invoice — Failure (tok_card_declined)

A failed payment does NOT change the invoice state — it stays `open` so the customer can retry with a different card. The payment attempt is recorded with the failure reason. This is important: PSP failures never corrupt invoice state.

Create and finalize a second invoice, then attempt payment with a declined token:

```bash
curl -s -X POST http://localhost:8080/v1/invoices/<invoice_id_2>/pay \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-002" \
  -d '{"card_token": "tok_card_declined"}' | jq
```

Returns 200 with `status: "failed"` and `failure_code: "card_declined"`. Invoice stays `open`.

### 8. Check Webhook Deliveries

Webhook delivery is fully decoupled from the API response — the handler returns immediately, and delivery happens via `tokio::spawn`. Each payload is HMAC-SHA256 signed so receivers can verify authenticity. Failed deliveries retry with exponential backoff (1m → 5m → 30m → 2h → 24h, max 5 attempts).

If you registered a webhook endpoint, you'll see events delivered for each state change:

- `invoice.created` (from step 3)
- `invoice.paid` (from step 5)
- `invoice.payment_failed` (from step 7)

Each delivery includes headers:

- `X-Webhook-Signature`: HMAC-SHA256 hex digest of the body
- `X-Webhook-Timestamp`: Unix timestamp
- `X-Webhook-Id`: Unique event UUID

You can also verify via service logs:

```bash
docker compose logs invoice-service | grep -i "webhook"
```

### 9. Pay Invoice — PSP Timeout (tok_timeout)

This demonstrates the key failure-mode handling. The mock PSP sleeps 30 seconds, but our client has a 5-second timeout. When the timeout fires: the payment stays `pending` (honest state — we don't know the outcome), the invoice stays `open` (not corrupted), and the caller gets 202 Accepted. No webhook is fired because no state change occurred.

```bash
curl -s -X POST http://localhost:8080/v1/invoices/<invoice_id_2>/pay \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-003" \
  -d '{"card_token": "tok_timeout"}' | jq
```

Returns **202 Accepted** with `status: "pending"` after ~5 seconds (client timeout). Invoice remains `open`.

---

## Invoice State Machine

```mermaid
stateDiagram-v2
    direction LR
    [*] --> Draft
    Draft --> Open : finalize
    Draft --> Void : void
    Open --> Paid : payment succeeded
    Open --> Void : void
    Open --> Uncollectible : mark uncollectible
```

**States**: Draft → Open → Paid | Void | Uncollectible

**Terminal states** (no transitions out): Paid, Void, Uncollectible

**Transitions**:
| From | To | Trigger |
|------|------|---------|
| Draft | Open | `POST /finalize` |
| Draft | Void | `POST /void` |
| Open | Paid | Successful PSP payment |
| Open | Void | `POST /void` |
| Open | Uncollectible | `POST /mark-uncollectible` |

**Invalid transitions** are rejected with 409 Conflict. Enforced at two levels:

1. Application: `InvoiceStatus::can_transition_to()` method
2. Database: `WHERE status = $current` in UPDATE queries (race-safe)

---

## Failure Mode: PSP Timeout (tok_timeout)

The Mock PSP delays 30 seconds for `tok_timeout`. The service handles this gracefully:

**How it works**:

1. Handler locks invoice row (`FOR UPDATE`), creates a `pending` payment attempt, commits
2. PSP client calls `POST /charge` with a **5-second timeout** (`reqwest` client timeout)
3. After 5s with no response → `reqwest` returns a timeout error
4. Handler catches the timeout → leaves payment as `pending`, does NOT update invoice
5. Returns **202 Accepted** to client with the pending payment attempt
6. Invoice stays in `open` state — not corrupted, not stuck
7. A **reconciliation worker** (runs every 60s) expires pending payments older than 10 minutes → invoice auto-unblocks for new attempts

**Why this design**:

- 5s timeout = 50x the normal PSP response (~100ms) — generous but bounded
- `pending` is an honest state — we don't know the outcome yet
- Invoice stays `open` so a retry can succeed
- No double-charge risk: same idempotency key returns same result on PSP side
- No permanently stuck invoices: reconciliation worker auto-expires stale pending payments (Razorpay-style TTL)

---

## Key Design Decisions

- **Money**: All amounts in integer cents (i64). No floats anywhere in the money path.
- **Concurrency**: Row-level `SELECT ... FOR UPDATE` locks prevent double-charging.
- **PSP Timeout**: 5s client timeout → returns 202 Accepted with pending status. Reconciliation worker expires stale payments after 10 min.
- **Webhooks**: Async delivery (`tokio::spawn`), HMAC-SHA256 signed, exponential backoff retry (5 attempts: 1m, 5m, 30m, 2h, 24h).
- **API Keys**: SHA-256 hashed in DB, 8-char prefix for identification, instant revocation via `revoked_at`.
- **Idempotency**: `UNIQUE(invoice_id, idempotency_key)` — same key + same body returns cached result; different body returns 409.

## Documentation

- [DESIGN.md](DESIGN.md) — Full design document (primary deliverable)
- [AI_USAGE.md](AI_USAGE.md) — AI tool usage disclosure
- [API Documentation](docs/API.md) — Full endpoint reference

## Demo Video

https://github.com/user-attachments/assets/c1849651-8ce3-4775-873d-ba28fb95c05b

[Watch the demo walkthrough on Google Drive](https://drive.google.com/file/d/1vq8i1J4_phB2UaprKQmdkeiXGJ3UcNpo/view?usp=sharing)

## Running Tests

```bash
# Requires PostgreSQL + mock-psp running (docker compose up postgres mock-psp -d)
cargo test --workspace
```

Key tests (74 total):

- **Concurrency**: 10 parallel payment requests → at most 1 succeeds, no double-charge
- **Idempotency**: Same key → same response without second PSP call; different body → 409
- **PSP failure**: `tok_timeout` and `tok_network_error` → returns 202 pending, invoice not stuck
