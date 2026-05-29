//! lib-core: Shared foundation crate for the Dodo Payments platform.
//!
//! Contains: configuration, error types, Result alias, and domain model types.
//! All service crates depend on this. This crate has NO dependency on service crates.

// region:    --- Modules

pub mod bmc;
pub mod config;
pub mod ctx;
pub mod error;
pub mod model;

// endregion: --- Modules

pub use error::{Error, Result};
