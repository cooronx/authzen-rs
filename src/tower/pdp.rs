use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
};

use bytes::Bytes;
use http::{HeaderValue, Method, Request, Response, StatusCode, header::CONTENT_TYPE};
use http_body::Body;
use http_body_util::{BodyExt, Limited};
use serde::{Serialize, de::DeserializeOwned};
use tower::Service;

use crate::{
    ActionSearchRequest, EvaluationRequest, EvaluationsRequest, PdpMetadata, ResourceSearchRequest,
    SubjectSearchRequest,
    server::{ActionSearch, PolicyDecisionPoint, ResourceSearch, SubjectSearch},
};

use super::body::{AuthZenBody, BoxError, full};

const DEFAULT_MAX_BODY: usize = 4 * 1024 * 1024;
const REQUEST_ID: &str = "x-request-id";
type ServiceFuture =
    Pin<Box<dyn Future<Output = Result<Response<AuthZenBody>, Infallible>> + Send>>;

pub trait PdpErrorMapper: Send + Sync + 'static {
    fn map(&self, error: &dyn std::error::Error) -> Response<AuthZenBody>;
}

impl<F> PdpErrorMapper for F
where
    F: Fn(&dyn std::error::Error) -> Response<AuthZenBody> + Send + Sync + 'static,
{
    fn map(&self, error: &dyn std::error::Error) -> Response<AuthZenBody> {
        self(error)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DefaultPdpErrorMapper;
impl PdpErrorMapper for DefaultPdpErrorMapper {
    fn map(&self, _: &dyn std::error::Error) -> Response<AuthZenBody> {
        text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal policy decision error",
            None,
        )
    }
}

macro_rules! service_type {
    ($name:ident, $bound:path) => {
        #[derive(Clone)]
        pub struct $name<P> {
            pdp: P,
            max_request_body_bytes: usize,
            error_mapper: Arc<dyn PdpErrorMapper>,
        }
        impl<P> $name<P> {
            pub fn new(pdp: P) -> Self {
                Self {
                    pdp,
                    max_request_body_bytes: DEFAULT_MAX_BODY,
                    error_mapper: Arc::new(DefaultPdpErrorMapper),
                }
            }
            pub fn max_request_body_bytes(mut self, bytes: usize) -> Self {
                self.max_request_body_bytes = bytes;
                self
            }
            pub fn error_mapper(mut self, mapper: impl PdpErrorMapper) -> Self {
                self.error_mapper = Arc::new(mapper);
                self
            }
        }
    };
}

service_type!(EvaluationService, PolicyDecisionPoint);
service_type!(EvaluationsService, PolicyDecisionPoint);
service_type!(SubjectSearchService, SubjectSearch);
service_type!(ResourceSearchService, ResourceSearch);
service_type!(ActionSearchService, ActionSearch);

impl<P, B> Service<Request<B>> for EvaluationService<P>
where
    P: PolicyDecisionPoint,
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError> + 'static,
{
    type Response = Response<AuthZenBody>;
    type Error = Infallible;
    type Future = ServiceFuture;
    fn poll_ready(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: Request<B>) -> Self::Future {
        let pdp = self.pdp.clone();
        let limit = self.max_request_body_bytes;
        let error_mapper = Arc::clone(&self.error_mapper);
        Box::pin(async move {
            let (request, request_id): (EvaluationRequest, _) =
                match parse_json(request, Method::POST, limit).await {
                    Ok(value) => value,
                    Err(response) => return Ok(response),
                };
            if let Err(error) = request.validate() {
                return Ok(text_response(
                    StatusCode::BAD_REQUEST,
                    error.to_string(),
                    request_id,
                ));
            }
            match pdp.evaluate(request).await {
                Ok(decision) => Ok(json_response(StatusCode::OK, &decision, request_id)),
                Err(error) => {
                    log_internal(&error);
                    Ok(map_pdp_error(error_mapper.as_ref(), &error, request_id))
                }
            }
        })
    }
}

impl<P, B> Service<Request<B>> for EvaluationsService<P>
where
    P: PolicyDecisionPoint,
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError> + 'static,
{
    type Response = Response<AuthZenBody>;
    type Error = Infallible;
    type Future = ServiceFuture;
    fn poll_ready(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: Request<B>) -> Self::Future {
        let pdp = self.pdp.clone();
        let limit = self.max_request_body_bytes;
        let error_mapper = Arc::clone(&self.error_mapper);
        Box::pin(async move {
            let (request, request_id): (EvaluationsRequest, _) =
                match parse_json(request, Method::POST, limit).await {
                    Ok(value) => value,
                    Err(response) => return Ok(response),
                };
            if let Err(error) = request.validate() {
                return Ok(text_response(
                    StatusCode::BAD_REQUEST,
                    error.to_string(),
                    request_id,
                ));
            }
            match pdp.evaluations(request).await {
                Ok(decisions) => Ok(json_response(StatusCode::OK, &decisions, request_id)),
                Err(error) => {
                    log_internal(&error);
                    Ok(map_pdp_error(error_mapper.as_ref(), &error, request_id))
                }
            }
        })
    }
}

macro_rules! impl_search_service {
    ($service:ident, $trait:path, $request:ty, $method:ident, $normalize:expr) => {
        impl<P, B> Service<Request<B>> for $service<P>
        where
            P: $trait,
            B: Body<Data = Bytes> + Send + 'static,
            B::Error: Into<BoxError> + 'static,
        {
            type Response = Response<AuthZenBody>;
            type Error = Infallible;
            type Future = ServiceFuture;
            fn poll_ready(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
            fn call(&mut self, request: Request<B>) -> Self::Future {
                let pdp = self.pdp.clone();
                let limit = self.max_request_body_bytes;
                let error_mapper = Arc::clone(&self.error_mapper);
                Box::pin(async move {
                    let (request, request_id): ($request, _) =
                        match parse_json(request, Method::POST, limit).await {
                            Ok(value) => value,
                            Err(response) => return Ok(response),
                        };
                    if let Err(error) = request.validate() {
                        return Ok(text_response(
                            StatusCode::BAD_REQUEST,
                            error.to_string(),
                            request_id,
                        ));
                    }
                    let request = ($normalize)(request);
                    match pdp.$method(request).await {
                        Ok(results) => Ok(json_response(StatusCode::OK, &results, request_id)),
                        Err(error) => {
                            log_internal(&error);
                            Ok(map_pdp_error(error_mapper.as_ref(), &error, request_id))
                        }
                    }
                })
            }
        }
    };
}

impl_search_service!(
    SubjectSearchService,
    SubjectSearch,
    SubjectSearchRequest,
    search_subjects,
    SubjectSearchRequest::normalize_query
);
impl_search_service!(
    ResourceSearchService,
    ResourceSearch,
    ResourceSearchRequest,
    search_resources,
    ResourceSearchRequest::normalize_query
);
impl_search_service!(
    ActionSearchService,
    ActionSearch,
    ActionSearchRequest,
    search_actions,
    |request| request
);

#[derive(Clone)]
pub struct MetadataService {
    metadata: PdpMetadata,
}
impl MetadataService {
    pub fn new(metadata: PdpMetadata) -> Self {
        Self { metadata }
    }
}

impl<B> Service<Request<B>> for MetadataService
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError> + 'static,
{
    type Response = Response<AuthZenBody>;
    type Error = Infallible;
    type Future = ServiceFuture;
    fn poll_ready(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: Request<B>) -> Self::Future {
        let metadata = self.metadata.clone();
        Box::pin(async move {
            let request_id = request.headers().get(REQUEST_ID).cloned();
            if request.method() != Method::GET {
                return Ok(text_response(
                    StatusCode::BAD_REQUEST,
                    "metadata endpoint requires GET",
                    request_id,
                ));
            }
            if let Err(error) = metadata.validate() {
                return Ok(text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error.to_string(),
                    request_id,
                ));
            }
            Ok(json_response(StatusCode::OK, &metadata, request_id))
        })
    }
}

async fn parse_json<T, B>(
    request: Request<B>,
    method: Method,
    limit: usize,
) -> Result<(T, Option<HeaderValue>), Response<AuthZenBody>>
where
    T: DeserializeOwned,
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError> + 'static,
{
    let request_id = request.headers().get(REQUEST_ID).cloned();
    if request.method() != method {
        return Err(text_response(
            StatusCode::BAD_REQUEST,
            "invalid HTTP method",
            request_id,
        ));
    }
    let is_json = request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(content_type_is_json);
    if !is_json {
        return Err(text_response(
            StatusCode::BAD_REQUEST,
            "Content-Type must be application/json",
            request_id,
        ));
    }
    let body = match Limited::new(request.into_body(), limit).collect().await {
        Ok(body) => body.to_bytes(),
        Err(_) => {
            return Err(text_response(
                StatusCode::BAD_REQUEST,
                "request body exceeds the configured limit or could not be read",
                request_id,
            ));
        }
    };
    serde_json::from_slice(&body)
        .map(|value| (value, request_id.clone()))
        .map_err(|error| text_response(StatusCode::BAD_REQUEST, error.to_string(), request_id))
}

fn json_response<T: Serialize>(
    status: StatusCode,
    value: &T,
    request_id: Option<HeaderValue>,
) -> Response<AuthZenBody> {
    match serde_json::to_vec(value) {
        Ok(body) => response(status, "application/json", body, request_id),
        Err(_) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Response serialization failed",
            request_id,
        ),
    }
}

fn text_response(
    status: StatusCode,
    value: impl Into<String>,
    request_id: Option<HeaderValue>,
) -> Response<AuthZenBody> {
    response(
        status,
        "text/plain; charset=utf-8",
        value.into().into_bytes(),
        request_id,
    )
}

fn response(
    status: StatusCode,
    content_type: &'static str,
    body: Vec<u8>,
    request_id: Option<HeaderValue>,
) -> Response<AuthZenBody> {
    let mut response = Response::new(full(body));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Some(value) = request_id {
        response.headers_mut().insert(REQUEST_ID, value);
    }
    response
}

fn content_type_is_json(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
}

fn map_pdp_error(
    mapper: &dyn PdpErrorMapper,
    error: &dyn std::error::Error,
    request_id: Option<HeaderValue>,
) -> Response<AuthZenBody> {
    let mut response = mapper.map(error);
    if let Some(value) = request_id {
        response.headers_mut().insert(REQUEST_ID, value);
    }
    response
}

fn log_internal(error: &dyn std::error::Error) {
    #[cfg(feature = "tracing")]
    tracing::error!(error = %error, "AuthZEN PDP evaluation failed");
    #[cfg(not(feature = "tracing"))]
    let _ = error;
}
