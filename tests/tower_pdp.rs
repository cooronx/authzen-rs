#![cfg(all(feature = "tower", feature = "server"))]

use std::{
    convert::Infallible,
    io,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use authzen_rs::{
    Action, ActionSearchRequest, Decision, EvaluationRequest, EvaluationsRequest,
    EvaluationsResponse, Resource, ResourceSearchRequest, SearchResponse, Subject,
    SubjectSearchRequest,
    server::{ActionSearch, PolicyDecisionPoint, ResourceSearch, SubjectSearch},
    tower::{
        ActionSearchService, EvaluationService, EvaluationsService, ResourceSearchService,
        SubjectSearchService,
    },
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

#[derive(Clone)]
struct InvalidEvaluations;

#[async_trait]
impl PolicyDecisionPoint for InvalidEvaluations {
    type Error = Infallible;

    async fn evaluate(&self, _: EvaluationRequest) -> Result<Decision, Self::Error> {
        Ok(Decision::new(true))
    }

    async fn evaluations(&self, _: EvaluationsRequest) -> Result<EvaluationsResponse, Self::Error> {
        Ok(serde_json::from_value(serde_json::json!({})).unwrap())
    }
}

#[tokio::test]
async fn evaluations_service_hides_invalid_adapter_responses_and_echoes_request_id() {
    let body = br#"{"subject":{"type":"user","id":"alice"},"action":{"name":"read"},"evaluations":[{"resource":{"type":"doc","id":"1"}}]}"#;
    let request = Request::post("/")
        .header("content-type", "application/json")
        .header("x-request-id", "batch-123")
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();
    let response = EvaluationsService::new(InvalidEvaluations)
        .oneshot(request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.headers()["x-request-id"], "batch-123");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        String::from_utf8_lossy(&body),
        "Internal policy decision error"
    );
}

#[derive(Clone)]
struct InvalidSearch;

#[async_trait]
impl SubjectSearch for InvalidSearch {
    type Error = Infallible;

    async fn search_subjects(
        &self,
        _: SubjectSearchRequest,
    ) -> Result<SearchResponse<Subject>, Self::Error> {
        Ok(serde_json::from_value(serde_json::json!({
            "results": [{"type": "user"}]
        }))
        .unwrap())
    }
}

#[async_trait]
impl ResourceSearch for InvalidSearch {
    type Error = Infallible;

    async fn search_resources(
        &self,
        _: ResourceSearchRequest,
    ) -> Result<SearchResponse<Resource>, Self::Error> {
        Ok(serde_json::from_value(serde_json::json!({
            "results": [{"id": "1"}]
        }))
        .unwrap())
    }
}

#[async_trait]
impl ActionSearch for InvalidSearch {
    type Error = Infallible;

    async fn search_actions(
        &self,
        _: ActionSearchRequest,
    ) -> Result<SearchResponse<Action>, Self::Error> {
        Ok(serde_json::from_value(serde_json::json!({
            "results": [{}]
        }))
        .unwrap())
    }
}

fn json_request(body: &'static [u8]) -> Request<Full<Bytes>> {
    Request::post("/")
        .header("content-type", "application/json")
        .header("x-request-id", "search-123")
        .body(Full::new(Bytes::from_static(body)))
        .unwrap()
}

#[tokio::test]
async fn every_search_service_rejects_invalid_adapter_results() {
    let subject = SubjectSearchService::new(InvalidSearch)
        .oneshot(json_request(
            br#"{"subject":{"type":"user"},"action":{"name":"read"},"resource":{"type":"doc","id":"1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(subject.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(subject.headers()["x-request-id"], "search-123");

    let resource = ResourceSearchService::new(InvalidSearch)
        .oneshot(json_request(
            br#"{"subject":{"type":"user","id":"alice"},"action":{"name":"read"},"resource":{"type":"doc"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resource.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(resource.headers()["x-request-id"], "search-123");

    let action = ActionSearchService::new(InvalidSearch)
        .oneshot(json_request(
            br#"{"subject":{"type":"user","id":"alice"},"resource":{"type":"doc","id":"1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(action.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(action.headers()["x-request-id"], "search-123");
}
