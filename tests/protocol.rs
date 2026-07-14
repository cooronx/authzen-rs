use authzen_rs::{
    Action, ActionSearchRequest, EvaluationRequest, EvaluationsRequest, PdpMetadata, Resource,
    ResourceSearchRequest, Subject, SubjectSearchRequest,
};
use serde_json::json;

// Fixtures in this file are based on the non-normative examples in:
// https://openid.github.io/authzen/#name-access-evaluation-api

#[test]
fn official_evaluation_example_round_trips() {
    let value = json!({
        "subject": {"type": "user", "id": "alice@example.com"},
        "resource": {"type": "account", "id": "123"},
        "action": {"name": "can_read", "properties": {"method": "GET"}},
        "context": {"time": "1985-10-26T01:22-07:00"},
        "unknown_future_field": true
    });
    let request: EvaluationRequest = serde_json::from_value(value).unwrap();
    request.validate().unwrap();
    let serialized = serde_json::to_value(request).unwrap();
    assert!(serialized.get("unknown_future_field").is_none());
}

#[test]
fn missing_required_id_is_a_semantic_error_but_empty_id_is_allowed() {
    let missing: EvaluationRequest = serde_json::from_value(json!({
        "subject": {"type": "user"}, "action": {"name": "read"},
        "resource": {"type": "document", "id": "1"}
    }))
    .unwrap();
    assert_eq!(missing.validate().unwrap_err().path(), "subject.id");

    EvaluationRequest::new(
        Subject::new("user", ""),
        Action::new("read"),
        Resource::new("document", ""),
    )
    .validate()
    .unwrap();
}

#[test]
fn search_requests_apply_request_specific_identity_rules() {
    SubjectSearchRequest::new(
        Subject::query("user"),
        Action::new("read"),
        Resource::new("doc", "1"),
    )
    .validate()
    .unwrap();
    ResourceSearchRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::query("doc"),
    )
    .validate()
    .unwrap();
    ActionSearchRequest::new(Subject::new("user", "alice"), Resource::new("doc", "1"))
        .validate()
        .unwrap();
}

#[test]
fn action_search_does_not_serialize_an_action() {
    let request =
        ActionSearchRequest::new(Subject::new("user", "alice"), Resource::new("doc", "1"));
    assert!(
        serde_json::to_value(request)
            .unwrap()
            .get("action")
            .is_none()
    );
}

#[test]
fn evaluations_resolve_top_level_defaults() {
    let request: EvaluationsRequest = serde_json::from_value(json!({
        "subject": {"type": "user", "id": "alice"},
        "action": {"name": "read"},
        "evaluations": [
            {"resource": {"type": "doc", "id": "1"}},
            {"resource": {"type": "doc", "id": "2"}}
        ]
    }))
    .unwrap();
    let resolved = request.resolved().unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[1].subject().unwrap().id(), Some("alice"));
}

#[test]
fn absent_evaluations_behaves_as_single_evaluation() {
    let request: EvaluationsRequest = serde_json::from_value(json!({
        "subject": {"type": "user", "id": "alice"},
        "action": {"name": "read"},
        "resource": {"type": "doc", "id": "1"}
    }))
    .unwrap();
    assert_eq!(request.resolved().unwrap().len(), 1);
}

#[test]
fn metadata_covers_registered_fields_and_preserves_signed_jwt() {
    let metadata: PdpMetadata = serde_json::from_value(json!({
        "policy_decision_point": "https://pdp.example.com",
        "access_evaluation_endpoint": "https://pdp.example.com/access/v1/evaluation",
        "access_evaluations_endpoint": "https://pdp.example.com/access/v1/evaluations",
        "search_subject_endpoint": "https://pdp.example.com/access/v1/search/subject",
        "search_resource_endpoint": "https://pdp.example.com/access/v1/search/resource",
        "search_action_endpoint": "https://pdp.example.com/access/v1/search/action",
        "capabilities": ["urn:example:capability"],
        "signed_metadata": "header.payload.signature"
    }))
    .unwrap();
    metadata.validate().unwrap();
    assert_eq!(metadata.signed_metadata(), Some("header.payload.signature"));
}
