use serde::{Deserialize, Serialize};

use crate::{
    ValidationError,
    model::{Action, Context, Decision, Resource, Subject, require},
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EvaluationRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject: Option<Subject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource: Option<Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
}

impl EvaluationRequest {
    pub fn new(subject: Subject, action: Action, resource: Resource) -> Self {
        Self {
            subject: Some(subject),
            action: Some(action),
            resource: Some(resource),
            context: None,
        }
    }
    pub fn subject(&self) -> Option<&Subject> {
        self.subject.as_ref()
    }
    pub fn action(&self) -> Option<&Action> {
        self.action.as_ref()
    }
    pub fn resource(&self) -> Option<&Resource> {
        self.resource.as_ref()
    }
    pub fn context(&self) -> Option<&Context> {
        self.context.as_ref()
    }
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        require(&self.subject, "subject")?;
        require(&self.action, "action")?;
        require(&self.resource, "resource")?;
        self.subject.as_ref().unwrap().validate_identified()?;
        self.action.as_ref().unwrap().validate()?;
        self.resource.as_ref().unwrap().validate_identified()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationsSemantic {
    #[default]
    ExecuteAll,
    DenyOnFirstDeny,
    PermitOnFirstPermit,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EvaluationOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evaluations_semantic: Option<EvaluationsSemantic>,
    #[serde(flatten)]
    additional: serde_json::Map<String, serde_json::Value>,
}

impl EvaluationOptions {
    pub fn new(semantic: EvaluationsSemantic) -> Self {
        Self {
            evaluations_semantic: Some(semantic),
            additional: Default::default(),
        }
    }
    pub fn semantic(&self) -> EvaluationsSemantic {
        self.evaluations_semantic.unwrap_or_default()
    }
    pub fn with_option(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.additional.insert(key.into(), value);
        }
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EvaluationsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject: Option<Subject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource: Option<Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evaluations: Vec<EvaluationRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    options: Option<EvaluationOptions>,
}

impl EvaluationsRequest {
    pub fn new(evaluations: Vec<EvaluationRequest>) -> Self {
        Self {
            evaluations,
            ..Self::default()
        }
    }
    pub fn evaluations(&self) -> &[EvaluationRequest] {
        &self.evaluations
    }
    pub fn subject(&self) -> Option<&Subject> {
        self.subject.as_ref()
    }
    pub fn action(&self) -> Option<&Action> {
        self.action.as_ref()
    }
    pub fn resource(&self) -> Option<&Resource> {
        self.resource.as_ref()
    }
    pub fn context(&self) -> Option<&Context> {
        self.context.as_ref()
    }
    pub fn options(&self) -> Option<&EvaluationOptions> {
        self.options.as_ref()
    }
    pub fn with_defaults(
        mut self,
        subject: Option<Subject>,
        action: Option<Action>,
        resource: Option<Resource>,
        context: Option<Context>,
    ) -> Self {
        self.subject = subject;
        self.action = action;
        self.resource = resource;
        self.context = context;
        self
    }
    pub fn with_options(mut self, options: EvaluationOptions) -> Self {
        self.options = Some(options);
        self
    }
    pub fn semantic(&self) -> EvaluationsSemantic {
        self.options
            .as_ref()
            .map(EvaluationOptions::semantic)
            .unwrap_or_default()
    }

    pub fn resolved(&self) -> Result<Vec<EvaluationRequest>, ValidationError> {
        if self.evaluations.is_empty() {
            let request = EvaluationRequest {
                subject: self.subject.clone(),
                action: self.action.clone(),
                resource: self.resource.clone(),
                context: self.context.clone(),
            };
            request.validate()?;
            return Ok(vec![request]);
        }
        self.evaluations
            .iter()
            .cloned()
            .map(|mut item| {
                if item.subject.is_none() {
                    item.subject = self.subject.clone();
                }
                if item.action.is_none() {
                    item.action = self.action.clone();
                }
                if item.resource.is_none() {
                    item.resource = self.resource.clone();
                }
                if item.context.is_none() {
                    item.context = self.context.clone();
                }
                item.validate()?;
                Ok(item)
            })
            .collect()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        self.resolved().map(|_| ())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EvaluationsResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decision: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evaluations: Vec<Decision>,
}

impl EvaluationsResponse {
    pub fn multiple(evaluations: Vec<Decision>) -> Self {
        Self {
            decision: None,
            context: None,
            evaluations,
        }
    }
    pub fn single(decision: Decision) -> Self {
        Self {
            decision: Some(decision.allowed()),
            context: decision.context().cloned(),
            evaluations: Vec::new(),
        }
    }
    pub fn decision(&self) -> Option<bool> {
        self.decision
    }
    pub fn evaluations(&self) -> &[Decision] {
        &self.evaluations
    }
    pub fn context(&self) -> Option<&Context> {
        self.context.as_ref()
    }

    /// Validates that this response has the shape and result sequence required
    /// by the corresponding evaluations request.
    pub fn validate(&self, request: &EvaluationsRequest) -> Result<(), ValidationError> {
        if request.evaluations().is_empty() {
            if self.decision.is_none() {
                return Err(ValidationError::new(
                    "decision",
                    "a single-compatible response requires a decision",
                ));
            }
            if !self.evaluations.is_empty() {
                return Err(ValidationError::new(
                    "evaluations",
                    "a single-compatible response must use the Decision shape",
                ));
            }
            return Ok(());
        }

        if self.decision.is_some() || self.context.is_some() {
            return Err(ValidationError::new(
                "decision",
                "a batch response must use the evaluations array",
            ));
        }

        let expected = request.evaluations().len();
        let actual = self.evaluations.len();
        if actual == 0 {
            return Err(ValidationError::new(
                "evaluations",
                "a batch response requires at least one decision",
            ));
        }
        if actual > expected {
            return Err(ValidationError::new(
                "evaluations",
                "response contains more decisions than the request",
            ));
        }

        match request.semantic() {
            EvaluationsSemantic::ExecuteAll if actual != expected => Err(ValidationError::new(
                "evaluations",
                "execute_all requires one decision per evaluation",
            )),
            EvaluationsSemantic::DenyOnFirstDeny => {
                validate_short_circuit(&self.evaluations, expected, false)
            }
            EvaluationsSemantic::PermitOnFirstPermit => {
                validate_short_circuit(&self.evaluations, expected, true)
            }
            EvaluationsSemantic::ExecuteAll => Ok(()),
        }
    }
}

fn validate_short_circuit(
    evaluations: &[Decision],
    expected: usize,
    terminal_decision: bool,
) -> Result<(), ValidationError> {
    let (last, preceding) = evaluations
        .split_last()
        .expect("batch response was checked as non-empty");
    if preceding
        .iter()
        .any(|decision| decision.allowed() == terminal_decision)
    {
        return Err(ValidationError::new(
            "evaluations",
            "response continued after the short-circuit decision",
        ));
    }
    if evaluations.len() < expected && last.allowed() != terminal_decision {
        return Err(ValidationError::new(
            "evaluations",
            "a partial response must end with the short-circuit decision",
        ));
    }
    Ok(())
}
