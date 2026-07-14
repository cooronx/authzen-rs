#![cfg(feature = "server")]

use std::convert::Infallible;

use async_trait::async_trait;
use authzen_rs::{
    Action, Decision, EvaluationOptions, EvaluationRequest, EvaluationsRequest,
    EvaluationsSemantic, Resource, Subject, server::PolicyDecisionPoint,
};

#[derive(Clone)]
struct TestPdp;

#[async_trait]
impl PolicyDecisionPoint for TestPdp {
    type Error = Infallible;
    async fn evaluate(&self, request: EvaluationRequest) -> Result<Decision, Self::Error> {
        Ok(Decision::new(
            request.resource().and_then(Resource::id) != Some("denied"),
        ))
    }
}

fn request(id: &str) -> EvaluationRequest {
    EvaluationRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::new("doc", id),
    )
}

#[tokio::test]
async fn default_batch_implementation_short_circuits() {
    let batch = EvaluationsRequest::new(vec![request("ok"), request("denied"), request("ok")])
        .with_options(EvaluationOptions::new(EvaluationsSemantic::DenyOnFirstDeny));
    let response = TestPdp.evaluations(batch).await.unwrap();
    assert_eq!(response.evaluations().len(), 2);
    assert!(!response.evaluations()[1].allowed());
}

#[tokio::test]
async fn missing_evaluations_returns_single_decision_shape() {
    let batch: EvaluationsRequest = serde_json::from_value(serde_json::json!({
        "subject": {"type": "user", "id": "alice"},
        "action": {"name": "read"},
        "resource": {"type": "doc", "id": "ok"}
    }))
    .unwrap();
    let response = TestPdp.evaluations(batch).await.unwrap();
    assert_eq!(response.decision(), Some(true));
    assert!(response.evaluations().is_empty());
}
