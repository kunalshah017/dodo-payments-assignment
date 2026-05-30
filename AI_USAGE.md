# AI Usage Disclosure

## Tools Used

- **GitHub Copilot (Claude Opus 4.6 via VS Code agent mode)**: Used throughout the project for code generation, debugging, refactoring, and documentation. All generated code was reviewed, tested, and corrected where necessary.

## Specific Usage

1. **Project scaffolding & architecture**: Copilot generated the initial Cargo workspace setup, Dockerfiles, docker-compose.yml, and crate structure. I directed the architecture decisions: workspace layout with `crates/libs/` + `crates/services/`, separation of mock PSP as its own binary, and the BMC (Backend Model Controller) data access pattern.

2. **Handler & model boilerplate**: Route handlers, model structs, SQLx queries, and error types were generated with Copilot assistance. Each was reviewed for correctness — particularly money handling (ensuring i64 cents throughout, checked arithmetic for overflow) and SQL parameterization (no string interpolation).

3. **Webhook implementation & refactoring**: The initial webhook dispatcher had a critical bug — it passed stale invoice data to the webhook payload (showing `status: "open"` on an `invoice.paid` event). I caught this during manual testing with webhook.site. Copilot proposed a local variable patch (`invoice.status = InvoiceStatus::Paid`), but I rejected that as fragile and directed it toward the transactional outbox pattern instead.

4. **Test generation**: Integration tests were AI-generated based on my specifications (concurrency test with 10 parallel requests, idempotency key conflict detection, PSP timeout handling). I verified each test actually exercised the intended behavior.

5. **Documentation**: DESIGN.md structure and initial prose were AI-assisted. All design decisions (state machine transitions, timeout values, retry intervals, concurrency mechanism choice) were mine.

## Three Decisions I Made Myself

1. **Transactional outbox over fire-and-forget webhooks**: Copilot's first webhook implementation used `tokio::spawn` with a re-read from DB. I identified two problems: (a) crash between state commit and event insertion = lost event, (b) re-reading introduces TOCTOU races. I directed the refactoring to insert webhook events in the same transaction as the state change — the outbox pattern used by Stripe and production billing systems. This required creating `_tx` variants of BMC methods and restructuring the payment handler.

2. **Workspace architecture (`libs/` + `services/` separation)**: Copilot scaffolded a generic flat directory with all code in one crate. I restructured into `crates/libs/` (lib-core, lib-auth) and `crates/services/` (invoice-service, mock-psp) to enforce a strict dependency direction: services → libs, never the reverse. This keeps domain logic reusable and testable without pulling in HTTP framework dependencies, and makes it trivial to add new services later without touching the core.

3. **Payment reconciliation worker (Razorpay-style TTL expiration)**: After implementing the lock-then-release-early pattern, I realized invoices could get permanently stuck if a PSP call timed out — the pending attempt blocked all new attempts indefinitely. Copilot had no awareness of this gap. I researched how Razorpay and Stripe handle this: Razorpay polls acquiring banks periodically. I directed the implementation of a background reconciliation worker that expires pending payments older than 10 minutes, plus a TTL clause in `has_active_attempt_tx` so the invoice auto-unblocks. In production, the worker would query the PSP's status endpoint instead of using pure TTL.

## One Thing the AI Got Wrong

The webhook dispatcher was passing the pre-mutation `invoice` struct to the webhook payload. After `InvoiceBmc::mark_paid()` updated the DB, the webhook still sent `"status": "open"` because it used the stale local variable fetched before the update.

Copilot's first fix was `invoice.status = InvoiceStatus::Paid` — manually syncing the local variable. This is exactly the kind of patch that creates bugs at scale: every future state change would need a manual sync, and forgetting one is invisible until production.

I pushed for the structural fix: make it impossible to pass stale data by design. The result was the transactional outbox where the payload is built from the `RETURNING *` row inside the same transaction — no manual syncing, no stale data, no lost events on crash. The AI needed three iterations of direction (local patch → re-read from DB → full outbox) before arriving at the correct production pattern.
