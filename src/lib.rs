//! Rust SDK for the OpenID AuthZEN Authorization API 1.0.
//!
//! Protocol types are always available. HTTP client, PDP server traits, and
//! Tower integrations are controlled by Cargo features.

pub mod error;
pub mod model;
pub mod prelude;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
#[cfg(all(feature = "tower", any(feature = "client", feature = "server")))]
pub mod tower;

pub use error::{AuthZenError, ValidationError};
pub use model::*;
