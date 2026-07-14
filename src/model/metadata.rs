use serde::{Deserialize, Serialize};
use url::Url;

use crate::ValidationError;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PdpMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy_decision_point: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_evaluation_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_evaluations_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_subject_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_resource_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_action_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signed_metadata: Option<String>,
}

impl PdpMetadata {
    pub fn new(
        policy_decision_point: impl Into<String>,
        access_evaluation_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            policy_decision_point: Some(policy_decision_point.into()),
            access_evaluation_endpoint: Some(access_evaluation_endpoint.into()),
            ..Self::default()
        }
    }
    pub fn policy_decision_point(&self) -> Option<&str> {
        self.policy_decision_point.as_deref()
    }
    pub fn access_evaluation_endpoint(&self) -> Option<&str> {
        self.access_evaluation_endpoint.as_deref()
    }
    pub fn access_evaluations_endpoint(&self) -> Option<&str> {
        self.access_evaluations_endpoint.as_deref()
    }
    pub fn search_subject_endpoint(&self) -> Option<&str> {
        self.search_subject_endpoint.as_deref()
    }
    pub fn search_resource_endpoint(&self) -> Option<&str> {
        self.search_resource_endpoint.as_deref()
    }
    pub fn search_action_endpoint(&self) -> Option<&str> {
        self.search_action_endpoint.as_deref()
    }
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    /// Raw JWT. v0.1 preserves but does not validate signed metadata.
    pub fn signed_metadata(&self) -> Option<&str> {
        self.signed_metadata.as_deref()
    }
    pub fn with_access_evaluations_endpoint(mut self, value: impl Into<String>) -> Self {
        self.access_evaluations_endpoint = Some(value.into());
        self
    }
    pub fn with_subject_search_endpoint(mut self, value: impl Into<String>) -> Self {
        self.search_subject_endpoint = Some(value.into());
        self
    }
    pub fn with_resource_search_endpoint(mut self, value: impl Into<String>) -> Self {
        self.search_resource_endpoint = Some(value.into());
        self
    }
    pub fn with_action_search_endpoint(mut self, value: impl Into<String>) -> Self {
        self.search_action_endpoint = Some(value.into());
        self
    }
    pub fn with_capability(mut self, value: impl Into<String>) -> Self {
        self.capabilities.push(value.into());
        self
    }
    pub fn with_signed_metadata(mut self, value: impl Into<String>) -> Self {
        self.signed_metadata = Some(value.into());
        self
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let pdp = self
            .policy_decision_point
            .as_deref()
            .ok_or(ValidationError::new(
                "policy_decision_point",
                "required field is missing",
            ))?;
        let url = Url::parse(pdp)
            .map_err(|_| ValidationError::new("policy_decision_point", "must be a valid URL"))?;
        if url.scheme() != "https" || url.query().is_some() || url.fragment().is_some() {
            return Err(ValidationError::new(
                "policy_decision_point",
                "must be an HTTPS URL without query or fragment",
            ));
        }
        let endpoint = self
            .access_evaluation_endpoint
            .as_deref()
            .ok_or(ValidationError::new(
                "access_evaluation_endpoint",
                "required field is missing",
            ))?;
        validate_endpoint(endpoint, "access_evaluation_endpoint")?;
        for (value, path) in [
            (
                &self.access_evaluations_endpoint,
                "access_evaluations_endpoint",
            ),
            (&self.search_subject_endpoint, "search_subject_endpoint"),
            (&self.search_resource_endpoint, "search_resource_endpoint"),
            (&self.search_action_endpoint, "search_action_endpoint"),
        ] {
            if let Some(value) = value {
                validate_endpoint(value, path)?;
            }
        }
        Ok(())
    }
}

fn validate_endpoint(value: &str, path: &'static str) -> Result<(), ValidationError> {
    let url = Url::parse(value).map_err(|_| ValidationError::new(path, "must be a valid URL"))?;
    if url.scheme() != "https" {
        return Err(ValidationError::new(path, "must use HTTPS"));
    }
    Ok(())
}
