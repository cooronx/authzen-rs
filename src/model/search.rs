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
}
