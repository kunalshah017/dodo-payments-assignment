# Invoice & Payment Service

A minimal Invoice & Payment Service built in **Rust** with **Axum**, **PostgreSQL**, and a mock PSP. Businesses create invoices for their customers, customers pay invoices, and businesses receive signed webhooks for state changes.

## Quick Start

```bash
docker compose up
```

That's it. This brings up:

- **PostgreSQL** (port 5432) — with auto-migrations
- **Mock PSP** (port 9090) — simulates payment processor
- **Invoice Service** (port 8080) — the main API

## Test API Key

Pre-seeded for development:

```
dodo_test_key_1234567890abcdef
```

## Curl Examples

### 1. Create a Customer

```bash
curl -X POST http://localhost:8080/v1/customers \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -d '{"name": "Jane Doe", "email": "jane@example.com"}'
```

### 2. Create an Invoice

```bash
curl -X POST http://localhost:8080/v1/invoices \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "<customer_id_from_step_1>",
    "due_date": "2025-02-01",
    "line_items": [
      {"description": "Consulting (1hr)", "quantity": 2, "unit_amount_cents": 15000},
      {"description": "Platform fee", "quantity": 1, "unit_amount_cents": 5000}
    ]
  }'
```

### 3. Finalize Invoice (draft → open)

```bash
curl -X POST http://localhost:8080/v1/invoices/<invoice_id>/finalize \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef"
```

### 4. Pay Invoice (success)

```bash
curl -X POST http://localhost:8080/v1/invoices/<invoice_id>/pay \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: unique-key-123" \
  -d '{"card_token": "tok_success"}'
```

### 5. Pay Invoice (failure — card declined)

```bash
curl -X POST http://localhost:8080/v1/invoices/<invoice_id>/pay \
  -H "Authorization: Bearer dodo_test_key_1234567890abcdef" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: unique-key-456" \
  -d '{"card_token": "tok_card_declined"}'
```

## Architecture

```
┌─────────────────┐     ┌──────────────┐     ┌──────────┐
│ Invoice Service │────▶│   Mock PSP   │     │ Postgres │
│   (Axum:8080)  │     │  (Axum:9090) │     │  (:5432) │
└────────┬────────┘     └──────────────┘     └────┬─────┘
         │                                        │
         └──────── SQLx (async) ──────────────────┘
```

## Key Design Decisions

- **Money**: All amounts in integer cents (i64). No floats.
- **Concurrency**: Row-level `FOR UPDATE` locks prevent double-charging.
- **PSP Timeout**: 5s client timeout → returns 202 Accepted with pending status.
- **Webhooks**: Async delivery, HMAC-SHA256 signed, exponential backoff (5 retries).
- **API Keys**: SHA-256 hashed, prefix-stored for identification, instant revocation.

## Documentation

- [DESIGN.md](DESIGN.md) — Full design document (primary deliverable)
- [AI_USAGE.md](AI_USAGE.md) — AI tool usage disclosure
- [API Documentation](API.md) — OpenAPI / endpoint reference

## Demo Video

> **TODO**: Add Loom/recording link here

## Running Tests

```bash
cargo test
```

Key tests:

- Concurrent payment (N requests → at most 1 succeeds)
- Idempotency (same key → same response, no duplicate PSP call)
- PSP failure (tok_timeout/tok_network_error → invoice not stuck)
