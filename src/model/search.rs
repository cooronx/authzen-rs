use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    ValidationError,
    model::{Action, Context, Resource, Subject, require},
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PageRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    properties: Map<String, Value>,
}

impl PageRequest {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
    pub fn limit(&self) -> Option<u64> {
        self.limit
    }
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }
    pub fn with_limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }
    pub fn properties(&self) -> &Map<String, Value> {
        &self.properties
    }
    pub fn with_property(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.properties.insert(key.into(), value);
        }
        self
    }

    #[cfg(feature = "client")]
    pub(crate) fn continuation(&self, token: impl Into<String>) -> Self {
        let mut page = self.clone();
        page.token = Some(token.into());
        page
    }
}

#[cfg(feature = "client")]
fn continuation_page(page: Option<&PageRequest>, token: impl Into<String>) -> PageRequest {
    let token = token.into();
    match page {
        Some(page) => page.continuation(token),
        None => PageRequest::new().with_token(token),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageResponse {
    next_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    properties: Map<String, Value>,
}

impl PageResponse {
    pub fn new(next_token: impl Into<String>) -> Self {
        Self {
            next_token: next_token.into(),
            count: None,
            total: None,
            properties: Map::new(),
        }
    }
    pub fn next_token(&self) -> &str {
        &self.next_token
    }
    pub fn count(&self) -> Option<u64> {
        self.count
    }
    pub fn total(&self) -> Option<u64> {
        self.total
    }
    pub fn with_count(mut self, count: u64) -> Self {
        self.count = Some(count);
        self
    }
    pub fn with_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }
    pub fn properties(&self) -> &Map<String, Value> {
        &self.properties
    }

    fn validate(&self, result_count: usize) -> Result<(), ValidationError> {
        let result_count = u64::try_from(result_count).map_err(|_| {
            ValidationError::new(
                "results",
                "result count cannot be represented by the protocol",
            )
        })?;
        if self.count.is_some_and(|count| count != result_count) {
            return Err(ValidationError::new(
                "page.count",
                "must equal the number of returned results",
            ));
        }
        if self.total.is_some_and(|total| total < result_count) {
            return Err(ValidationError::new(
                "page.total",
                "cannot be smaller than the number of returned results",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchResponse<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page: Option<PageResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
    results: Vec<T>,
}

impl<T> SearchResponse<T> {
    pub fn new(results: Vec<T>) -> Self {
        Self {
            page: None,
            context: None,
            results,
        }
    }
    pub fn results(&self) -> &[T] {
        &self.results
    }
    pub fn page(&self) -> Option<&PageResponse> {
        self.page.as_ref()
    }
    pub fn next_token(&self) -> Option<&str> {
        self.page
            .as_ref()
            .map(PageResponse::next_token)
            .filter(|value| !value.is_empty())
    }
    pub fn context(&self) -> Option<&Context> {
        self.context.as_ref()
    }
    pub fn with_page(mut self, page: PageResponse) -> Self {
        self.page = Some(page);
        self
    }
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }

    fn validate_with(
        &self,
        request_page: Option<&PageRequest>,
        validate_result: impl Fn(&T) -> Result<(), ValidationError>,
    ) -> Result<(), ValidationError> {
        if request_page
            .and_then(PageRequest::limit)
            .is_some_and(|limit| {
                u64::try_from(self.results.len()).map_or(true, |count| count > limit)
            })
        {
            return Err(ValidationError::new(
                "results",
                "contains more items than the requested page limit",
            ));
        }
        if let Some(page) = &self.page {
            page.validate(self.results.len())?;
        }
        self.results.iter().try_for_each(validate_result)
    }
}

impl SearchResponse<Subject> {
    /// Validates a Subject Search response against its request.
    pub fn validate(&self, request: &SubjectSearchRequest) -> Result<(), ValidationError> {
        self.validate_with(request.page(), Subject::validate_identified)
    }
}

impl SearchResponse<Resource> {
    /// Validates a Resource Search response against its request.
    pub fn validate(&self, request: &ResourceSearchRequest) -> Result<(), ValidationError> {
        self.validate_with(request.page(), Resource::validate_identified)
    }
}

impl SearchResponse<Action> {
    /// Validates an Action Search response against its request.
    pub fn validate(&self, request: &ActionSearchRequest) -> Result<(), ValidationError> {
        self.validate_with(request.page(), Action::validate)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubjectSearchRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject: Option<Subject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource: Option<Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page: Option<PageRequest>,
}

impl SubjectSearchRequest {
    pub fn new(subject: Subject, action: Action, resource: Resource) -> Self {
        Self {
            subject: Some(subject),
            action: Some(action),
            resource: Some(resource),
            context: None,
            page: None,
        }
    }
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
    pub fn with_page(mut self, page: PageRequest) -> Self {
        self.page = Some(page);
        self
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
    pub fn page(&self) -> Option<&PageRequest> {
        self.page.as_ref()
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        require(&self.subject, "subject")?;
        require(&self.action, "action")?;
        require(&self.resource, "resource")?;
        self.subject.as_ref().unwrap().validate_query()?;
        self.action.as_ref().unwrap().validate()?;
        self.resource.as_ref().unwrap().validate_identified()
    }
    #[cfg(feature = "client")]
    pub(crate) fn continuation(&self, token: impl Into<String>) -> Self {
        let mut request = self.clone();
        request.page = Some(continuation_page(request.page.as_ref(), token));
        request
    }
    #[cfg(all(feature = "tower", feature = "server"))]
    pub(crate) fn normalize_query(mut self) -> Self {
        self.subject.as_mut().unwrap().ignore_id();
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourceSearchRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject: Option<Subject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource: Option<Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page: Option<PageRequest>,
}

impl ResourceSearchRequest {
    pub fn new(subject: Subject, action: Action, resource: Resource) -> Self {
        Self {
            subject: Some(subject),
            action: Some(action),
            resource: Some(resource),
            context: None,
            page: None,
        }
    }
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
    pub fn with_page(mut self, page: PageRequest) -> Self {
        self.page = Some(page);
        self
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
    pub fn page(&self) -> Option<&PageRequest> {
        self.page.as_ref()
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        require(&self.subject, "subject")?;
        require(&self.action, "action")?;
        require(&self.resource, "resource")?;
        self.subject.as_ref().unwrap().validate_identified()?;
        self.action.as_ref().unwrap().validate()?;
        self.resource.as_ref().unwrap().validate_query()
    }
    #[cfg(feature = "client")]
    pub(crate) fn continuation(&self, token: impl Into<String>) -> Self {
        let mut request = self.clone();
        request.page = Some(continuation_page(request.page.as_ref(), token));
        request
    }
    #[cfg(all(feature = "tower", feature = "server"))]
    pub(crate) fn normalize_query(mut self) -> Self {
        self.resource.as_mut().unwrap().ignore_id();
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionSearchRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject: Option<Subject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource: Option<Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page: Option<PageRequest>,
}

impl ActionSearchRequest {
    pub fn new(subject: Subject, resource: Resource) -> Self {
        Self {
            subject: Some(subject),
            resource: Some(resource),
            context: None,
            page: None,
        }
    }
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
    pub fn with_page(mut self, page: PageRequest) -> Self {
        self.page = Some(page);
        self
    }
    pub fn subject(&self) -> Option<&Subject> {
        self.subject.as_ref()
    }
    pub fn resource(&self) -> Option<&Resource> {
        self.resource.as_ref()
    }
    pub fn context(&self) -> Option<&Context> {
        self.context.as_ref()
    }
    pub fn page(&self) -> Option<&PageRequest> {
        self.page.as_ref()
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        require(&self.subject, "subject")?;
        require(&self.resource, "resource")?;
        self.subject.as_ref().unwrap().validate_identified()?;
        self.resource.as_ref().unwrap().validate_identified()
    }
    #[cfg(feature = "client")]
    pub(crate) fn continuation(&self, token: impl Into<String>) -> Self {
        let mut request = self.clone();
        request.page = Some(continuation_page(request.page.as_ref(), token));
        request
    }
}
