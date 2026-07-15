use std::error::Error;

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::{
    Action, ActionSearchRequest, Decision, EvaluationsRequest, EvaluationsResponse,
    EvaluationsSemantic, Resource, ResourceSearchRequest, SearchResponse, Subject,
    SubjectSearchRequest,
};

pub trait PdpError: Error + Send + Sync + 'static {}
impl<T> PdpError for T where T: Error + Send + Sync + 'static {}

#[async_trait]
pub trait PolicyDecisionPoint: Clone + Send + Sync + 'static {
    type Error: PdpError;

    async fn evaluate(&self, request: crate::EvaluationRequest) -> Result<Decision, Self::Error>;

    async fn evaluations(
        &self,
        request: EvaluationsRequest,
    ) -> Result<EvaluationsResponse, Self::Error> {
        let had_multiple = !request.evaluations().is_empty();
        let semantic = request.semantic();
        let requests = match request.resolved() {
            Ok(requests) => requests,
            Err(_) => return Ok(EvaluationsResponse::multiple(vec![evaluation_failure()])),
        };
        let mut decisions = Vec::with_capacity(requests.len());
        for request in requests {
            let decision = self
                .evaluate(request)
                .await
                .unwrap_or_else(|_| evaluation_failure());
            let stop = match semantic {
                EvaluationsSemantic::ExecuteAll => false,
                EvaluationsSemantic::DenyOnFirstDeny => !decision.allowed(),
                EvaluationsSemantic::PermitOnFirstPermit => decision.allowed(),
            };
            decisions.push(decision);
            if stop {
                break;
            }
        }
        if had_multiple {
            Ok(EvaluationsResponse::multiple(decisions))
        } else {
            Ok(EvaluationsResponse::single(decisions.remove(0)))
        }
    }
}

fn evaluation_failure() -> Decision {
    let mut error = Map::new();
    error.insert(
        "message".into(),
        Value::String("Policy evaluation failed".into()),
    );
    let mut context = Map::new();
    context.insert("error".into(), Value::Object(error));
    Decision::new(false).with_context(context)
}

#[async_trait]
pub trait SubjectSearch: Clone + Send + Sync + 'static {
    type Error: PdpError;

    /// Returns authorized subjects matching the request.
    ///
    /// When `request.page().token()` is present, the adapter must bind that
    /// opaque token to the original query and reject changes to every request
    /// value other than the token. The generic server transport deliberately
    /// does not retain pagination sessions.
    async fn search_subjects(
        &self,
        request: SubjectSearchRequest,
    ) -> Result<SearchResponse<Subject>, Self::Error>;
}

#[async_trait]
pub trait ResourceSearch: Clone + Send + Sync + 'static {
    type Error: PdpError;

    /// Returns authorized resources matching the request.
    ///
    /// When `request.page().token()` is present, the adapter must bind that
    /// opaque token to the original query and reject changes to every request
    /// value other than the token. The generic server transport deliberately
    /// does not retain pagination sessions.
    async fn search_resources(
        &self,
        request: ResourceSearchRequest,
    ) -> Result<SearchResponse<Resource>, Self::Error>;
}

#[async_trait]
pub trait ActionSearch: Clone + Send + Sync + 'static {
    type Error: PdpError;

    /// Returns authorized actions matching the request.
    ///
    /// When `request.page().token()` is present, the adapter must bind that
    /// opaque token to the original query and reject changes to every request
    /// value other than the token. The generic server transport deliberately
    /// does not retain pagination sessions.
    async fn search_actions(
        &self,
        request: ActionSearchRequest,
    ) -> Result<SearchResponse<Action>, Self::Error>;
}
