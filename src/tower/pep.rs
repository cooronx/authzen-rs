use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use http::{
    Request, Response, StatusCode,
    header::{CONTENT_TYPE, HeaderValue},
    request::Parts,
};
use http_body::Body;
use thiserror::Error;
use tower::{Layer, Service};

use crate::{AuthZenError, Decision, EvaluationRequest, client::Authorizer};

use super::body::{AuthZenBody, BoxError, boxed, full};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RequestMappingError {
    #[error("authentication identity is missing")]
    Unauthenticated,
    #[error("request cannot be mapped to an AuthZEN evaluation: {0}")]
    InvalidRequest(String),
}

pub trait RequestMapper: Clone + Send + Sync + 'static {
    fn map(&self, parts: &Parts) -> Result<EvaluationRequest, RequestMappingError>;
}

impl<F> RequestMapper for F
where
    F: Fn(&Parts) -> Result<EvaluationRequest, RequestMappingError> + Clone + Send + Sync + 'static,
{
    fn map(&self, parts: &Parts) -> Result<EvaluationRequest, RequestMappingError> {
        self(parts)
    }
}

#[derive(Clone, Debug, Default)]
#[doc(hidden)]
pub struct MissingMapper;
impl RequestMapper for MissingMapper {
    fn map(&self, _: &Parts) -> Result<EvaluationRequest, RequestMappingError> {
        Err(RequestMappingError::InvalidRequest(
            "request mapper is not configured".into(),
        ))
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum MiddlewareFailure {
    Mapping(RequestMappingError),
    Denied(Decision),
    Authorizer(AuthZenError),
    Timeout,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum AuditEvent<'a> {
    Allowed(&'a Decision),
    Denied(&'a Decision),
    Failed(&'a MiddlewareFailure),
}

pub trait AuditHook: Send + Sync + 'static {
    fn record(&self, event: AuditEvent<'_>);
}

impl<F> AuditHook for F
where
    F: for<'a> Fn(AuditEvent<'a>) + Send + Sync + 'static,
{
    fn record(&self, event: AuditEvent<'_>) {
        self(event)
    }
}

#[derive(Debug, Default)]
struct NoopAuditHook;
impl AuditHook for NoopAuditHook {
    fn record(&self, _: AuditEvent<'_>) {}
}

pub trait ResponseMapper: Clone + Send + Sync + 'static {
    fn map(&self, failure: &MiddlewareFailure) -> Response<AuthZenBody>;
}

impl<F> ResponseMapper for F
where
    F: Fn(&MiddlewareFailure) -> Response<AuthZenBody> + Clone + Send + Sync + 'static,
{
    fn map(&self, failure: &MiddlewareFailure) -> Response<AuthZenBody> {
        self(failure)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DefaultResponseMapper;
impl ResponseMapper for DefaultResponseMapper {
    fn map(&self, failure: &MiddlewareFailure) -> Response<AuthZenBody> {
        let (status, message) = match failure {
            MiddlewareFailure::Mapping(RequestMappingError::Unauthenticated) => {
                (StatusCode::UNAUTHORIZED, "Unauthenticated")
            }
            MiddlewareFailure::Mapping(RequestMappingError::InvalidRequest(_)) => {
                (StatusCode::BAD_REQUEST, "Invalid authorization request")
            }
            MiddlewareFailure::Denied(_) => (StatusCode::FORBIDDEN, "Forbidden"),
            MiddlewareFailure::Timeout => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Policy decision point unavailable",
            ),
            MiddlewareFailure::Authorizer(AuthZenError::InvalidResponse(_)) => (
                StatusCode::BAD_GATEWAY,
                "Invalid policy decision point response",
            ),
            MiddlewareFailure::Authorizer(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Policy decision point unavailable",
            ),
        };
        let mut response = Response::new(full(message));
        *response.status_mut() = status;
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        response
    }
}

#[derive(Clone)]
pub struct AuthZenLayer<A, M = MissingMapper, R = DefaultResponseMapper> {
    authorizer: A,
    mapper: M,
    response_mapper: R,
    timeout: Duration,
    insert_decision: bool,
    audit_hook: Arc<dyn AuditHook>,
}

impl<A> AuthZenLayer<A> {
    pub fn new(authorizer: A) -> Self {
        Self {
            authorizer,
            mapper: MissingMapper,
            response_mapper: DefaultResponseMapper,
            timeout: Duration::from_secs(5),
            insert_decision: true,
            audit_hook: Arc::new(NoopAuditHook),
        }
    }
}

impl<A, M, R> AuthZenLayer<A, M, R> {
    pub fn request_mapper<N>(self, mapper: N) -> AuthZenLayer<A, N, R> {
        AuthZenLayer {
            authorizer: self.authorizer,
            mapper,
            response_mapper: self.response_mapper,
            timeout: self.timeout,
            insert_decision: self.insert_decision,
            audit_hook: self.audit_hook,
        }
    }
    pub fn response_mapper<N>(self, response_mapper: N) -> AuthZenLayer<A, M, N> {
        AuthZenLayer {
            authorizer: self.authorizer,
            mapper: self.mapper,
            response_mapper,
            timeout: self.timeout,
            insert_decision: self.insert_decision,
            audit_hook: self.audit_hook,
        }
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn insert_decision(mut self, insert: bool) -> Self {
        self.insert_decision = insert;
        self
    }
    pub fn audit_hook(mut self, hook: impl AuditHook) -> Self {
        self.audit_hook = Arc::new(hook);
        self
    }
}

impl<S, A, M, R> Layer<S> for AuthZenLayer<A, M, R>
where
    A: Clone,
    M: Clone,
    R: Clone,
{
    type Service = AuthZenService<S, A, M, R>;
    fn layer(&self, inner: S) -> Self::Service {
        AuthZenService {
            inner,
            authorizer: self.authorizer.clone(),
            mapper: self.mapper.clone(),
            response_mapper: self.response_mapper.clone(),
            timeout: self.timeout,
            insert_decision: self.insert_decision,
            audit_hook: Arc::clone(&self.audit_hook),
        }
    }
}

#[derive(Clone)]
pub struct AuthZenService<S, A, M, R> {
    inner: S,
    authorizer: A,
    mapper: M,
    response_mapper: R,
    timeout: Duration,
    insert_decision: bool,
    audit_hook: Arc<dyn AuditHook>,
}

impl<S, A, M, R, B, RB> Service<Request<B>> for AuthZenService<S, A, M, R>
where
    S: Service<Request<B>, Response = Response<RB>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    A: Authorizer,
    M: RequestMapper,
    R: ResponseMapper,
    B: Send + 'static,
    RB: Body<Data = Bytes> + Send + 'static,
    RB::Error: Into<BoxError> + 'static,
{
    type Response = Response<AuthZenBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let replacement = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, replacement);
        let authorizer = self.authorizer.clone();
        let mapper = self.mapper.clone();
        let response_mapper = self.response_mapper.clone();
        let timeout = self.timeout;
        let insert_decision = self.insert_decision;
        let audit_hook = Arc::clone(&self.audit_hook);
        let future = async move {
            let (mut parts, body) = request.into_parts();
            let evaluation = match mapper.map(&parts) {
                Ok(request) => request,
                Err(error) => {
                    let failure = MiddlewareFailure::Mapping(error);
                    audit_hook.record(AuditEvent::Failed(&failure));
                    return Ok(response_mapper.map(&failure));
                }
            };
            let decision =
                match tokio::time::timeout(timeout, authorizer.evaluate(evaluation)).await {
                    Err(_) => {
                        let failure = MiddlewareFailure::Timeout;
                        audit_hook.record(AuditEvent::Failed(&failure));
                        return Ok(response_mapper.map(&failure));
                    }
                    Ok(Err(error)) => {
                        let failure = MiddlewareFailure::Authorizer(error);
                        audit_hook.record(AuditEvent::Failed(&failure));
                        return Ok(response_mapper.map(&failure));
                    }
                    Ok(Ok(decision)) => decision,
                };
            if !decision.allowed() {
                audit_hook.record(AuditEvent::Denied(&decision));
                return Ok(response_mapper.map(&MiddlewareFailure::Denied(decision)));
            }
            audit_hook.record(AuditEvent::Allowed(&decision));
            if insert_decision {
                parts.extensions.insert(decision);
            }
            let response = inner.call(Request::from_parts(parts, body)).await?;
            let (parts, body) = response.into_parts();
            Ok(Response::from_parts(parts, boxed(body)))
        };
        #[cfg(feature = "tracing")]
        {
            Box::pin(tracing::Instrument::instrument(
                future,
                tracing::info_span!("authzen.authorize"),
            ))
        }
        #[cfg(not(feature = "tracing"))]
        {
            Box::pin(future)
        }
    }
}
