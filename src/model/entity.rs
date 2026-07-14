use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ValidationError, model::require};

pub type Properties = Map<String, Value>;
pub type Context = Map<String, Value>;

/// A principal about whom authorization is requested.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Subject {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    subject_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    properties: Properties,
}

impl Subject {
    pub fn new(subject_type: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            subject_type: Some(subject_type.into()),
            id: Some(id.into()),
            properties: Map::new(),
        }
    }

    pub fn query(subject_type: impl Into<String>) -> Self {
        Self {
            subject_type: Some(subject_type.into()),
            id: None,
            properties: Map::new(),
        }
    }

    pub fn subject_type(&self) -> Option<&str> {
        self.subject_type.as_deref()
    }
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    pub fn properties(&self) -> &Properties {
        &self.properties
    }
    pub fn properties_mut(&mut self) -> &mut Properties {
        &mut self.properties
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.properties.insert(key.into(), value);
        }
        self
    }

    pub fn validate_identified(&self) -> Result<(), ValidationError> {
        require(&self.subject_type, "subject.type")?;
        require(&self.id, "subject.id")
    }

    pub fn validate_query(&self) -> Result<(), ValidationError> {
        require(&self.subject_type, "subject.type")
    }

    #[cfg(all(feature = "tower", feature = "server"))]
    pub(crate) fn ignore_id(&mut self) {
        self.id = None;
    }
}

/// The target of an access request.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    properties: Properties,
}

impl Resource {
    pub fn new(resource_type: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            resource_type: Some(resource_type.into()),
            id: Some(id.into()),
            properties: Map::new(),
        }
    }

    pub fn query(resource_type: impl Into<String>) -> Self {
        Self {
            resource_type: Some(resource_type.into()),
            id: None,
            properties: Map::new(),
        }
    }

    pub fn resource_type(&self) -> Option<&str> {
        self.resource_type.as_deref()
    }
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    pub fn properties(&self) -> &Properties {
        &self.properties
    }
    pub fn properties_mut(&mut self) -> &mut Properties {
        &mut self.properties
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.properties.insert(key.into(), value);
        }
        self
    }

    pub fn validate_identified(&self) -> Result<(), ValidationError> {
        require(&self.resource_type, "resource.type")?;
        require(&self.id, "resource.id")
    }

    pub fn validate_query(&self) -> Result<(), ValidationError> {
        require(&self.resource_type, "resource.type")
    }

    #[cfg(all(feature = "tower", feature = "server"))]
    pub(crate) fn ignore_id(&mut self) {
        self.id = None;
    }
}

/// The operation the subject intends to perform.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Action {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    properties: Properties,
}

impl Action {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            properties: Map::new(),
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn properties(&self) -> &Properties {
        &self.properties
    }
    pub fn properties_mut(&mut self) -> &mut Properties {
        &mut self.properties
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.properties.insert(key.into(), value);
        }
        self
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        require(&self.name, "action.name")
    }
}

/// An authorization decision returned by a PDP.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    decision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<Context>,
}

impl Decision {
    pub fn new(decision: bool) -> Self {
        Self {
            decision,
            context: None,
        }
    }
    pub fn allowed(&self) -> bool {
        self.decision
    }
    pub fn context(&self) -> Option<&Context> {
        self.context.as_ref()
    }
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
}
