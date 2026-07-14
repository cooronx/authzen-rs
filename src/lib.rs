//! Rust SDK for the OpenID AuthZEN Authorization API 1.0.
//!
//! Protocol types are always available. HTTP client, PDP server traits, and
//! Tower integrations are controlled by Cargo features.

pub mod error;
pub mod model;
pub mod prelude;

pub use error::{AuthZenError, ValidationError};
pub use model::*;
