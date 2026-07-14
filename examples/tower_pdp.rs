use std::convert::Infallible;

use async_trait::async_trait;
use authzen_rs::{
    Decision, EvaluationRequest, server::PolicyDecisionPoint, tower::EvaluationService,
};
use bytes::Bytes;
use http::Request;
use http_body_util::{BodyExt, Full};
use tower::ServiceExt;

#[derive(Clone)]
struct MyPdp;

#[async_trait]
impl PolicyDecisionPoint for MyPdp {
    type Error = Infallible;

    async fn evaluate(&self, _: EvaluationRequest) -> Result<Decision, Self::Error> {
        Ok(Decision::new(true))
    }
}

#[tokio::main]
async fn main() {
    let body = br#"{"subject":{"type":"user","id":"alice"},"action":{"name":"read"},"resource":{"type":"document","id":"123"}}"#;
    let request = Request::post("/access/v1/evaluation")
        .header("content-type", "application/json")
        .header("x-request-id", "example-request")
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();

    let response = EvaluationService::new(MyPdp)
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let request_id = response.headers()["x-request-id"].clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();

    println!(
        "status={} request_id={} body={}",
        status.as_u16(),
        request_id.to_str().unwrap(),
        String::from_utf8_lossy(&body)
    );
}
