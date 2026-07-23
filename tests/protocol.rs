use authzen_rs::{
    Action, ActionSearchRequest, Decision, EvaluationOptions, EvaluationRequest,
    EvaluationsRequest, EvaluationsResponse, EvaluationsSemantic, PageRequest, PdpMetadata,
    Resource, ResourceSearchRequest, SearchResponse, Subject, SubjectSearchRequest,
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
fn single_compatible_evaluations_require_the_decision_shape() {
    let request: EvaluationsRequest = serde_json::from_value(json!({
        "subject": {"type": "user", "id": "alice"},
        "action": {"name": "read"},
        "resource": {"type": "doc", "id": "1"}
    }))
    .unwrap();

    let missing: EvaluationsResponse = serde_json::from_value(json!({})).unwrap();
    assert_eq!(missing.validate(&request).unwrap_err().path(), "decision");

    let batch: EvaluationsResponse = serde_json::from_value(json!({
        "evaluations": [{"decision": true}]
    }))
    .unwrap();
    assert_eq!(batch.validate(&request).unwrap_err().path(), "decision");

    let complete: EvaluationsResponse = serde_json::from_value(json!({
        "decision": true,
        "unknown_future_field": "ignored"
    }))
    .unwrap();
    complete.validate(&request).unwrap();
}

fn batch_request(semantic: EvaluationsSemantic) -> EvaluationsRequest {
    EvaluationsRequest::new(vec![
        EvaluationRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::new("doc", "1"),
        ),
        EvaluationRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::new("doc", "2"),
        ),
        EvaluationRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::new("doc", "3"),
        ),
    ])
    .with_options(EvaluationOptions::new(semantic))
}

fn decisions(values: &[bool]) -> EvaluationsResponse {
    EvaluationsResponse::multiple(values.iter().copied().map(Decision::new).collect())
}

#[test]
fn execute_all_requires_one_ordered_result_per_evaluation() {
    let request = batch_request(EvaluationsSemantic::ExecuteAll);
    decisions(&[true, false, true]).validate(&request).unwrap();
    assert!(decisions(&[true, false]).validate(&request).is_err());
    assert!(
        decisions(&[true, false, true, true])
            .validate(&request)
            .is_err()
    );

    let malformed: Result<EvaluationsResponse, _> = serde_json::from_value(json!({
        "evaluations": [{"decision": true}, {}]
    }));
    assert!(malformed.is_err());
}

#[test]
fn short_circuit_responses_are_non_empty_valid_prefixes() {
    let deny = batch_request(EvaluationsSemantic::DenyOnFirstDeny);
    decisions(&[true, false]).validate(&deny).unwrap();
    decisions(&[true, true, true]).validate(&deny).unwrap();
    assert!(decisions(&[]).validate(&deny).is_err());
    assert!(decisions(&[true]).validate(&deny).is_err());
    assert!(decisions(&[false, true]).validate(&deny).is_err());

    let permit = batch_request(EvaluationsSemantic::PermitOnFirstPermit);
    decisions(&[false, true]).validate(&permit).unwrap();
    decisions(&[false, false, false]).validate(&permit).unwrap();
    assert!(decisions(&[false]).validate(&permit).is_err());
    assert!(decisions(&[true, false]).validate(&permit).is_err());
}

#[test]
fn search_responses_validate_all_three_entity_types() {
    let subject_request = SubjectSearchRequest::new(
        Subject::query("user"),
        Action::new("read"),
        Resource::new("doc", "1"),
    );
    let subjects: SearchResponse<Subject> = serde_json::from_value(json!({
        "results": [{"type": "user"}]
    }))
    .unwrap();
    assert_eq!(
        subjects.validate(&subject_request).unwrap_err().path(),
        "subject.id"
    );

    let resource_request = ResourceSearchRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::query("doc"),
    );
    let resources: SearchResponse<Resource> = serde_json::from_value(json!({
        "results": [{"id": "1"}]
    }))
    .unwrap();
    assert_eq!(
        resources.validate(&resource_request).unwrap_err().path(),
        "resource.type"
    );

    let action_request =
        ActionSearchRequest::new(Subject::new("user", "alice"), Resource::new("doc", "1"));
    let actions: SearchResponse<Action> = serde_json::from_value(json!({
        "results": [{}]
    }))
    .unwrap();
    assert_eq!(
        actions.validate(&action_request).unwrap_err().path(),
        "action.name"
    );
}

#[test]
fn search_responses_require_results_and_consistent_pagination() {
    assert!(
        serde_json::from_value::<SearchResponse<Resource>>(json!({
            "page": {"next_token": ""}
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SearchResponse<Resource>>(json!({
            "page": {},
            "results": []
        }))
        .is_err()
    );

    let request = ResourceSearchRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::query("doc"),
    )
    .with_page(PageRequest::new().with_limit(1));
    let count_mismatch: SearchResponse<Resource> = serde_json::from_value(json!({
        "page": {"next_token": "", "count": 2},
        "results": [{"type": "doc", "id": "1"}]
    }))
    .unwrap();
    assert_eq!(
        count_mismatch.validate(&request).unwrap_err().path(),
        "page.count"
    );

    let too_many: SearchResponse<Resource> = serde_json::from_value(json!({
        "page": {"next_token": "", "total": 1},
        "results": [
            {"type": "doc", "id": "1"},
            {"type": "doc", "id": "2"}
        ],
        "unknown_future_field": true
    }))
    .unwrap();
    assert_eq!(too_many.validate(&request).unwrap_err().path(), "results");

    let valid: SearchResponse<Resource> = serde_json::from_value(json!({
        "page": {"next_token": "", "count": 1, "total": 1, "future": true},
        "results": [{"type": "doc", "id": "1", "future": true}],
        "future": true
    }))
    .unwrap();
    valid.validate(&request).unwrap();
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
