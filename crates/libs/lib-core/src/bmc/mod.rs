//! Backend Model Controllers (BMC) — the data access layer.
//!
//! Each entity gets a BMC struct with consistent CRUD methods:
//! - `create` — insert a new record
//! - `get` — fetch by ID (scoped to business)
//! - `list` — fetch all records (scoped to business, with optional filters)
//!
//! Special operations (state transitions, payments) live as separate methods
//! on the relevant BMC.

pub mod customer;
pub mod invoice;
pub mod payment;
pub mod webhook;
