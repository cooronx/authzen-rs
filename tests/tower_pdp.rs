#![cfg(feature = "tower-pdp")]

use std::{
    convert::Infallible,
    io,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use authzen_rs::{
    Decision, EvaluationRequest, SearchResponse, Subject, SubjectSearchRequest,
    server::{PolicyDecisionPoint, SubjectSearch},
    tower::{EvaluationService, SubjectSearchService},
};
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::{BodyExt, Full};
use tower::ServiceExt;

#[derive(Clone)]
struct Allow;

#[async_trait]
impl PolicyDecisionPoint for Allow {
    type Error = Infallible;
    async fn evaluate(&self, _: EvaluationRequest) -> Result<Decision, Self::Error> {
        Ok(Decision::new(true))
    }
}

#[tokio::test]
async fn evaluation_service_handles_json_and_echoes_request_id() {
    let request = Request::post("/access/v1/evaluation")
        .header("content-type", "Application/JSON; Charset=UTF-8")
        .header("x-request-id", "abc")
        .body(Full::new(Bytes::from_static(br#"{"subject":{"type":"user","id":"alice"},"action":{"name":"read"},"resource":{"type":"doc","id":"1"}}"#)))
        .unwrap();
    let response = EvaluationService::new(Allow)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-request-id"], "abc");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"decision": true})
    );
}

#[tokio::test]
async fn evaluation_service_rejects_oversized_body() {
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from_static(b"{}")))
        .unwrap();
    let response = EvaluationService::new(Allow)
        .max_request_body_bytes(1)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[derive(Clone)]
struct CaptureSearch(Arc<Mutex<Option<String>>>);

#[async_trait]
impl SubjectSearch for CaptureSearch {
    type Error = Infallible;
    async fn search_subjects(
        &self,
        request: SubjectSearchRequest,
    ) -> Result<SearchResponse<Subject>, Self::Error> {
        *self.0.lock().unwrap() = request.subject().and_then(Subject::id).map(str::to_owned);
        Ok(SearchResponse::new(vec![]))
    }
}

#[tokio::test]
async fn subject_search_service_ignores_queried_subject_id() {
    let captured = Arc::new(Mutex::new(Some("not called".into())));
    let body = br#"{"subject":{"type":"user","id":"must-be-ignored"},"action":{"name":"read"},"resource":{"type":"doc","id":"1"}}"#;
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();
    let response = SubjectSearchService::new(CaptureSearch(Arc::clone(&captured)))
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(*captured.lock().unwrap(), None);
}

#[derive(Clone)]
struct FailingPdp;

#[async_trait]
impl PolicyDecisionPoint for FailingPdp {
    type Error = io::Error;
    async fn evaluate(&self, _: EvaluationRequest) -> Result<Decision, Self::Error> {
        Err(io::Error::other("secret database detail"))
    }
}

#[tokio::test]
async fn pdp_errors_are_hidden_by_default() {
    let body = br#"{"subject":{"type":"user","id":"alice"},"action":{"name":"read"},"resource":{"type":"doc","id":"1"}}"#;
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();
    let response = EvaluationService::new(FailingPdp)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&body).contains("secret database detail"));
}
