use thiserror::Error;

/// A protocol validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{path}: {message}")]
pub struct ValidationError {
    path: &'static str,
    message: &'static str,
}

impl ValidationError {
    pub const fn new(path: &'static str, message: &'static str) -> Self {
        Self { path, message }
    }

    pub const fn path(&self) -> &'static str {
        self.path
    }

    pub const fn message(&self) -> &'static str {
        self.message
    }
}

/// Errors produced by the SDK client and protocol layer.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuthZenError {
    #[error("invalid AuthZEN request: {0}")]
    InvalidRequest(#[from] ValidationError),
    #[cfg(feature = "client")]
    #[error("HTTP transport error: {0}")]
    Transport(#[source] reqwest::Error),
    #[error("request timed out")]
    Timeout,
    #[error("metadata discovery failed: {0}")]
    Discovery(String),
    #[error("invalid PDP metadata: {0}")]
    InvalidMetadata(String),
    #[error("PDP does not advertise the requested endpoint")]
    UnsupportedEndpoint,
    #[error("invalid PDP response: {0}")]
    InvalidResponse(String),
    #[error("PDP returned HTTP {status}: {message}")]
    Pdp { status: u16, message: String },
}
