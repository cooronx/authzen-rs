use std::convert::Infallible;

use async_trait::async_trait;
use authzen_rs::prelude::*;
use bytes::Bytes;
use http::{Request, Response, request::Parts};
use http_body_util::{BodyExt, Full};
use tower::{ServiceBuilder, ServiceExt, service_fn};

#[derive(Clone)]
struct LocalAuthorizer;

#[async_trait]
impl Authorizer for LocalAuthorizer {
    async fn evaluate(&self, request: EvaluationRequest) -> Result<Decision, AuthZenError> {
        Ok(Decision::new(
            request.resource().and_then(Resource::id) == Some("public"),
        ))
    }
}

fn map_request(parts: &Parts) -> Result<EvaluationRequest, RequestMappingError> {
    let document_id = parts.uri.path().trim_start_matches("/documents/");
    Ok(EvaluationRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::new("document", document_id),
    ))
}

async fn call(document_id: &str) {
    let inner = service_fn(|request: Request<()>| async move {
        let decision = request.extensions().get::<Decision>().unwrap();
        Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(format!(
            "handler called, allowed={}",
            decision.allowed()
        )))))
    });
    let service = ServiceBuilder::new()
        .layer(AuthZenLayer::new(LocalAuthorizer).request_mapper(map_request))
        .service(inner);
    let request = Request::get(format!("/documents/{document_id}"))
        .body(())
        .unwrap();
    let response = service.oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    println!(
        "{document_id} status={} body={}",
        status.as_u16(),
        String::from_utf8_lossy(&body)
    );
}

#[tokio::main]
async fn main() {
    call("public").await;
    call("private").await;
}
