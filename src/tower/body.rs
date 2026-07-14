use std::convert::Infallible;

use bytes::Bytes;
#[cfg(feature = "client")]
use http_body::Body;
use http_body_util::{BodyExt, Full, combinators::UnsyncBoxBody};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub type AuthZenBody = UnsyncBoxBody<Bytes, BoxError>;

pub fn full(value: impl Into<Bytes>) -> AuthZenBody {
    Full::new(value.into())
        .map_err(|error: Infallible| match error {})
        .boxed_unsync()
}

#[cfg(feature = "client")]
pub fn boxed<B>(body: B) -> AuthZenBody
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError> + 'static,
{
    body.map_err(Into::into).boxed_unsync()
}
