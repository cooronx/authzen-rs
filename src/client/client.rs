use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use futures_util::StreamExt;
pub use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{
    Client, Method, RequestBuilder,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap},
};
use serde::{Serialize, de::DeserializeOwned};
use url::Url;

use crate::{
    Action, ActionSearchRequest, AuthZenError, Decision, EvaluationRequest, EvaluationsRequest,
    EvaluationsResponse, PdpMetadata, Resource, ResourceSearchRequest, SearchResponse, Subject,
    SubjectSearchRequest,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_MAX_BODY: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApiEndpoint {
    Evaluation,
    Evaluations,
    SubjectSearch,
    ResourceSearch,
    ActionSearch,
}

impl ApiEndpoint {
    fn default_path(self) -> &'static str {
        match self {
            Self::Evaluation => "/access/v1/evaluation",
            Self::Evaluations => "/access/v1/evaluations",
            Self::SubjectSearch => "/access/v1/search/subject",
            Self::ResourceSearch => "/access/v1/search/resource",
            Self::ActionSearch => "/access/v1/search/action",
        }
    }
}

#[async_trait]
pub trait Authorizer: Clone + Send + Sync + 'static {
    async fn evaluate(&self, request: EvaluationRequest) -> Result<Decision, AuthZenError>;
}

#[derive(Clone)]
pub struct AuthZenClient {
    http: Client,
    endpoints: HashMap<ApiEndpoint, Url>,
    headers: HeaderMap,
    timeout: Duration,
    max_response_body_bytes: usize,
}

impl AuthZenClient {
    pub fn builder(policy_decision_point: impl Into<String>) -> AuthZenClientBuilder {
        AuthZenClientBuilder::new(policy_decision_point)
    }

    pub async fn evaluate(&self, request: EvaluationRequest) -> Result<Decision, AuthZenError> {
        request.validate()?;
        self.post(ApiEndpoint::Evaluation, &request).await
    }

    pub async fn evaluations(
        &self,
        request: EvaluationsRequest,
    ) -> Result<EvaluationsResponse, AuthZenError> {
        request.validate()?;
        self.post(ApiEndpoint::Evaluations, &request).await
    }

    pub async fn search_subjects(
        &self,
        request: SubjectSearchRequest,
    ) -> Result<SearchResponse<Subject>, AuthZenError> {
        request.validate()?;
        self.post(ApiEndpoint::SubjectSearch, &request).await
    }

    pub async fn search_resources(
        &self,
        request: ResourceSearchRequest,
    ) -> Result<SearchResponse<Resource>, AuthZenError> {
        request.validate()?;
        self.post(ApiEndpoint::ResourceSearch, &request).await
    }

    pub async fn search_actions(
        &self,
        request: ActionSearchRequest,
    ) -> Result<SearchResponse<Action>, AuthZenError> {
        request.validate()?;
        self.post(ApiEndpoint::ActionSearch, &request).await
    }

    async fn post<T: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        endpoint: ApiEndpoint,
        body: &T,
    ) -> Result<R, AuthZenError> {
        let url = self
            .endpoints
            .get(&endpoint)
            .ok_or(AuthZenError::UnsupportedEndpoint)?;
        let request = self
            .http
            .post(url.clone())
            .headers(self.headers.clone())
            .json(body);
        self.execute_json(request).await
    }

    async fn execute_json<R: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<R, AuthZenError> {
        let response = tokio::time::timeout(self.timeout, request.send())
            .await
            .map_err(|_| AuthZenError::Timeout)?
            .map_err(AuthZenError::Transport)?;
        let status = response.status();
        let is_json = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(content_type_is_json);
        let bytes = read_limited(response, self.max_response_body_bytes).await?;
        if !status.is_success() {
            return Err(AuthZenError::Pdp {
                status: status.as_u16(),
                message: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        if status != reqwest::StatusCode::OK {
            return Err(AuthZenError::InvalidResponse(format!(
                "successful response must use HTTP 200, received {status}"
            )));
        }
        if !is_json {
            return Err(AuthZenError::InvalidResponse(
                "successful response must use Content-Type application/json".into(),
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| AuthZenError::InvalidResponse(error.to_string()))
    }
}

#[async_trait]
impl Authorizer for AuthZenClient {
    async fn evaluate(&self, request: EvaluationRequest) -> Result<Decision, AuthZenError> {
        AuthZenClient::evaluate(self, request).await
    }
}

pub struct AuthZenClientBuilder {
    policy_decision_point: String,
    http: Option<Client>,
    headers: HeaderMap,
    timeout: Duration,
    max_response_body_bytes: usize,
    discover: bool,
    endpoints: HashMap<ApiEndpoint, String>,
    error: Option<AuthZenError>,
}

impl AuthZenClientBuilder {
    pub fn new(policy_decision_point: impl Into<String>) -> Self {
        Self {
            policy_decision_point: policy_decision_point.into(),
            http: None,
            headers: HeaderMap::new(),
            timeout: DEFAULT_TIMEOUT,
            max_response_body_bytes: DEFAULT_MAX_BODY,
            discover: false,
            endpoints: HashMap::new(),
            error: None,
        }
    }

    pub fn discover(mut self) -> Self {
        self.discover = true;
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn max_response_body_bytes(mut self, bytes: usize) -> Self {
        self.max_response_body_bytes = bytes;
        self
    }
    pub fn http_client(mut self, client: Client) -> Self {
        self.http = Some(client);
        self
    }

    pub fn bearer_token(mut self, token: impl AsRef<str>) -> Self {
        match HeaderValue::from_str(&format!("Bearer {}", token.as_ref())) {
            Ok(mut value) => {
                value.set_sensitive(true);
                self.headers.insert(AUTHORIZATION, value);
            }
            Err(_) => {
                self.error = Some(AuthZenError::InvalidRequest(crate::ValidationError::new(
                    "authorization",
                    "invalid bearer token",
                )))
            }
        }
        self
    }

    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }
    pub fn endpoint(mut self, endpoint: ApiEndpoint, url: impl Into<String>) -> Self {
        self.endpoints.insert(endpoint, url.into());
        self
    }

    pub async fn build(self) -> Result<AuthZenClient, AuthZenError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let base = Url::parse(&self.policy_decision_point)
            .map_err(|error| AuthZenError::InvalidMetadata(error.to_string()))?;
        if base.scheme() != "https" || base.query().is_some() || base.fragment().is_some() {
            return Err(AuthZenError::InvalidMetadata(
                "policy decision point must be an HTTPS URL without query or fragment".into(),
            ));
        }
        let http = match self.http {
            Some(client) => client,
            None => Client::builder()
                .timeout(self.timeout)
                .build()
                .map_err(AuthZenError::Transport)?,
        };
        let endpoints = if self.discover {
            discover_endpoints(
                &http,
                &base,
                &self.headers,
                self.timeout,
                self.max_response_body_bytes,
            )
            .await?
        } else {
            default_endpoints(&base, self.endpoints)?
        };
        Ok(AuthZenClient {
            http,
            endpoints,
            headers: self.headers,
            timeout: self.timeout,
            max_response_body_bytes: self.max_response_body_bytes,
        })
    }
}

fn default_endpoints(
    base: &Url,
    overrides: HashMap<ApiEndpoint, String>,
) -> Result<HashMap<ApiEndpoint, Url>, AuthZenError> {
    let mut endpoints = HashMap::new();
    for endpoint in [
        ApiEndpoint::Evaluation,
        ApiEndpoint::Evaluations,
        ApiEndpoint::SubjectSearch,
        ApiEndpoint::ResourceSearch,
        ApiEndpoint::ActionSearch,
    ] {
        let url = match overrides.get(&endpoint) {
            Some(url) => {
                Url::parse(url).map_err(|error| AuthZenError::InvalidMetadata(error.to_string()))?
            }
            None => append_default_path(base, endpoint.default_path()),
        };
        if url.scheme() != "https" {
            return Err(AuthZenError::InvalidMetadata(
                "endpoint must use HTTPS".into(),
            ));
        }
        endpoints.insert(endpoint, url);
    }
    Ok(endpoints)
}

fn append_default_path(base: &Url, suffix: &str) -> Url {
    let mut url = base.clone();
    let base_path = base.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{}", suffix.trim_start_matches('/')));
    url
}

async fn discover_endpoints(
    http: &Client,
    base: &Url,
    headers: &HeaderMap,
    timeout: Duration,
    max_body: usize,
) -> Result<HashMap<ApiEndpoint, Url>, AuthZenError> {
    let mut discovery = base.clone();
    let original_path = base.path().trim_start_matches('/');
    discovery.set_path(&if original_path.is_empty() {
        "/.well-known/authzen-configuration".into()
    } else {
        format!("/.well-known/authzen-configuration/{original_path}")
    });
    discovery.set_query(base.query());
    let response = tokio::time::timeout(
        timeout,
        http.request(Method::GET, discovery)
            .headers(headers.clone())
            .send(),
    )
    .await
    .map_err(|_| AuthZenError::Timeout)?
    .map_err(|error| AuthZenError::Discovery(error.to_string()))?;
    if !response.status().is_success() {
        return Err(AuthZenError::Discovery(format!(
            "metadata endpoint returned {}",
            response.status()
        )));
    }
    if response.status() != reqwest::StatusCode::OK {
        return Err(AuthZenError::InvalidMetadata(format!(
            "metadata response must use HTTP 200, received {}",
            response.status()
        )));
    }
    let is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(content_type_is_json);
    if !is_json {
        return Err(AuthZenError::InvalidMetadata(
            "metadata response must use Content-Type application/json".into(),
        ));
    }
    let bytes = read_limited(response, max_body).await?;
    let metadata: PdpMetadata = serde_json::from_slice(&bytes)
        .map_err(|error| AuthZenError::InvalidMetadata(error.to_string()))?;
    metadata
        .validate()
        .map_err(|error| AuthZenError::InvalidMetadata(error.to_string()))?;
    let actual = Url::parse(metadata.policy_decision_point().unwrap())
        .map_err(|error| AuthZenError::InvalidMetadata(error.to_string()))?;
    if actual != *base {
        return Err(AuthZenError::InvalidMetadata(
            "policy_decision_point does not match the discovery identifier".into(),
        ));
    }
    let values = [
        (
            ApiEndpoint::Evaluation,
            metadata.access_evaluation_endpoint(),
        ),
        (
            ApiEndpoint::Evaluations,
            metadata.access_evaluations_endpoint(),
        ),
        (
            ApiEndpoint::SubjectSearch,
            metadata.search_subject_endpoint(),
        ),
        (
            ApiEndpoint::ResourceSearch,
            metadata.search_resource_endpoint(),
        ),
        (ApiEndpoint::ActionSearch, metadata.search_action_endpoint()),
    ];
    values
        .into_iter()
        .filter_map(|(kind, value)| value.map(|value| (kind, value)))
        .map(|(kind, value)| {
            let url = Url::parse(value)
                .map_err(|error| AuthZenError::InvalidMetadata(error.to_string()))?;
            Ok((kind, url))
        })
        .collect()
}

async fn read_limited(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, AuthZenError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AuthZenError::InvalidResponse(format!(
            "response body exceeds {limit} bytes"
        )));
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(AuthZenError::Transport)?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AuthZenError::InvalidResponse(format!(
                "response body exceeds {limit} bytes"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn content_type_is_json(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
}
