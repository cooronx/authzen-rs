mod entity;
mod evaluation;
mod metadata;
mod search;

pub use entity::{Action, Context, Decision, Properties, Resource, Subject};
pub use evaluation::{
    EvaluationOptions, EvaluationRequest, EvaluationsRequest, EvaluationsResponse,
    EvaluationsSemantic,
};
pub use metadata::PdpMetadata;
pub use search::{
    ActionSearchRequest, PageRequest, PageResponse, ResourceSearchRequest, SearchResponse,
    SubjectSearchRequest,
};

pub(crate) fn require<T>(
    value: &Option<T>,
    path: &'static str,
) -> Result<(), crate::ValidationError> {
    if value.is_some() {
        Ok(())
    } else {
        Err(crate::ValidationError::new(
            path,
            "required field is missing",
        ))
    }
}
