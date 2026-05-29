---
description: General project instructions for the Dodo Payments Invoice & Payment Service
applyTo: "**/*"
---

# Dodo Payments - Invoice & Payment Service

## Project Overview

A production-grade Invoice & Payment Service built in Rust (Axum framework) for the Dodo Payments backend engineering challenge. The system handles business authentication, customer management, invoicing with line items, payment processing via a mock PSP, and webhook delivery.

## Tech Stack

- **Language**: Rust (edition 2021, MSRV 1.80)
- **Web Framework**: Axum 0.8 (tokio-rs)
- **Async Runtime**: Tokio
- **Database**: PostgreSQL 16 via SQLx (async, compile-time checked queries)
- **HTTP Client**: Reqwest (for PSP calls)
- **Middleware**: Tower + tower-http (tracing, timeouts, CORS)
- **Crypto**: SHA-256 (API key hashing), HMAC-SHA256 (webhook signing)

## Project Structure

```
├── Cargo.toml                      # Workspace root
├── crates/
│   ├── libs/
│   │   ├── lib-core/               # Foundation: config, errors, models, BMCs
│   │   │   └── src/
│   │   │       ├── lib.rs          # Crate root, re-exports Error & Result
│   │   │       ├── config.rs       # OnceLock environment configuration
│   │   │       ├── ctx.rs          # Request context (authenticated business ID)
│   │   │       ├── error.rs        # Unified Error enum + Result type alias
│   │   │       ├── model/          # Domain types & DTOs (From impls for conversions)
│   │   │       └── bmc/            # Backend Model Controllers (data access layer)
│   │   └── lib-auth/               # Authentication: middleware, API key hashing
│   │       └── src/
│   │           ├── lib.rs
│   │           ├── middleware.rs    # mw_auth (Axum middleware, inserts Ctx)
│   │           └── token.rs        # hash_api_key (SHA-256)
│   └── services/
│       ├── invoice-service/         # Main API server (port 8080)
│       │   └── src/
│       │       ├── main.rs          # Entry point, server setup
│       │       ├── routes/          # HTTP handlers (thin: validate → BMC → respond)
│       │       └── services/        # External integrations (PSP client, webhooks)
│       └── mock-psp/                # Mock Payment Service Provider (port 9090)
├── migrations/                      # PostgreSQL migrations (SQLx)
├── docker-compose.yml               # One-command setup
└── docs/                            # DESIGN.md, AI_USAGE.md, API docs
```

## Coding Conventions

### Architecture Patterns

- **Crate dependency direction**: `services → libs`. Never the reverse. `lib-auth → lib-core`. Never circular.
- **OnceLock config**: Configuration loaded once via `lib_core::config::config()`. No passing config around.
- **Ctx (request context)**: Every authenticated request carries a `Ctx` via Axum extensions. Handlers extract `Extension(ctx): Extension<Ctx>`.
- **BMC (Backend Model Controller)**: All database access goes through BMC structs in `lib-core::bmc`. One BMC per entity. Consistent interface: `create`, `get`, `list` + special operations as explicit methods.
- **Route handlers are thin**: Validate input → call BMC → map to response. No SQL in route handlers.
- **Error type**: Single `lib_core::Error` enum used everywhere. Per-crate `Result<T>` type alias via `pub use error::{Error, Result};`.
- **Model DTOs**: Request types suffixed `Create`/`Update`, response types suffixed `Response`. Implement `From<Model> for Response` for clean conversions.
- **Region comments**: Use `// region: --- Section` / `// endregion: --- Section` for code organization.

### Rust Style

- Follow standard `rustfmt` formatting
- Use `clippy` with default lints
- Prefer `thiserror` for library-style errors, `anyhow` for application-level
- All money values are `i64` representing **cents** (integer minor units). NEVER use floats for money.
- Use `Uuid` (v4) for all primary keys
- Use `chrono::DateTime<Utc>` for timestamps

### API Design

- All endpoints return consistent JSON error format: `{"error": {"type": "...", "message": "..."}}`
- Authentication via `Authorization: Bearer <api_key>` header
- Idempotency via `Idempotency-Key` header on payment endpoints
- HTTP status codes: 201 (created), 200 (success), 202 (accepted/pending), 4xx (client errors), 5xx (server errors)

### Database

- Use SQLx migrations in `migrations/` directory
- Naming: snake_case tables, columns
- Always use parameterized queries (never string interpolation for SQL values)
- Use `FOR UPDATE` row locks for payment concurrency control
- Status-conditional UPDATEs to prevent race conditions

### Error Handling

- Never expose internal errors to clients
- Log internal errors with `tracing::error!`
- Map database errors to appropriate HTTP status codes
- PSP failures should NOT corrupt invoice state

### Testing

- Integration tests use the real database (test containers)
- Key tests: concurrency, idempotency, PSP failure handling
- Use `tokio::test` for async tests

### Commits

- Use conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
- Each commit should be atomic and self-contained
- Write clear commit messages explaining WHY, not just WHAT

## Key Design Decisions

1. **Invoice State Machine**: draft → open → paid | void | uncollectible
2. **Payment Concurrency**: Row-level locking (`SELECT ... FOR UPDATE`) prevents double-charges
3. **PSP Timeout Handling**: 5-second client timeout; payment stays "pending" on timeout; caller gets 202 Accepted
4. **Webhook Delivery**: Async (tokio::spawn), HMAC-SHA256 signed, exponential backoff retry (5 attempts max)
5. **API Key Security**: SHA-256 hashed in DB, never stored as plaintext, 8-char prefix for identification

## What NOT to Build

- Subscriptions, recurring billing, plans
- Refunds or partial payments
- Multi-currency or FX
- Tax calculation
- Frontend/UI
- Email sending (log only)
- Rate limiting (discuss in DESIGN.md)
- OAuth (API keys only)
