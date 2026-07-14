#![cfg(all(feature = "tower", feature = "client"))]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use authzen_rs::{
    Action, AuthZenError, Decision, EvaluationRequest, Resource, Subject,
    client::Authorizer,
    tower::{AuditEvent, AuthZenLayer, RequestMappingError},
};
use bytes::Bytes;
use http::{Request, Response, StatusCode, request::Parts};
use http_body_util::Full;
use tower::{ServiceBuilder, ServiceExt, service_fn};

#[derive(Clone)]
struct FixedAuthorizer(bool);

#[async_trait]
impl Authorizer for FixedAuthorizer {
    async fn evaluate(&self, _: EvaluationRequest) -> Result<Decision, AuthZenError> {
        Ok(Decision::new(self.0))
    }
}

fn mapper(_: &Parts) -> Result<EvaluationRequest, RequestMappingError> {
    Ok(EvaluationRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::new("doc", "1"),
    ))
}

#[tokio::test]
async fn allowed_request_calls_inner_service_and_inserts_decision() {
    let audit_count = Arc::new(AtomicUsize::new(0));
    let audit_count_for_hook = Arc::clone(&audit_count);
    let inner = service_fn(|request: Request<()>| async move {
        assert!(request.extensions().get::<Decision>().unwrap().allowed());
        Ok::<_, std::convert::Infallible>(Response::new(Full::new(Bytes::from_static(b"ok"))))
    });
    let service = ServiceBuilder::new()
        .layer(
            AuthZenLayer::new(FixedAuthorizer(true))
                .request_mapper(mapper)
                .audit_hook(move |_: AuditEvent<'_>| {
                    audit_count_for_hook.fetch_add(1, Ordering::Relaxed);
                }),
        )
        .service(inner);
    let response = service.oneshot(Request::new(())).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(audit_count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn denied_request_does_not_call_inner_service() {
    let inner = service_fn(|_: Request<()>| async move {
        panic!("inner service must not be called");
        #[allow(unreachable_code)]
        Ok::<_, std::convert::Infallible>(Response::new(Full::new(Bytes::new())))
    });
    let service = ServiceBuilder::new()
        .layer(AuthZenLayer::new(FixedAuthorizer(false)).request_mapper(mapper))
        .service(inner);
    let response = service.oneshot(Request::new(())).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
