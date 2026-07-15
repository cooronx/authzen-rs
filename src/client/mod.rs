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

macro_rules! search_paginator {
    ($name:ident, $request:ty, $result:ty, $search:ident) => {
        /// Stateful Search pagination that keeps the initial query fixed and
        /// changes only the opaque continuation token between requests.
        #[must_use = "a paginator does not send a request until next_page is called"]
        pub struct $name {
            client: AuthZenClient,
            request: $request,
            finished: bool,
        }

        impl $name {
            fn new(client: AuthZenClient, request: $request) -> Self {
                Self {
                    client,
                    request,
                    finished: false,
                }
            }

            /// Returns the next page, or `None` after a response omits `page`
            /// or supplies an empty `next_token`.
            ///
            /// A failed request does not advance the token, so callers may
            /// retry by calling this method again.
            pub async fn next_page(
                &mut self,
            ) -> Result<Option<SearchResponse<$result>>, AuthZenError> {
                if self.finished {
                    return Ok(None);
                }
                let response = self.client.$search(self.request.clone()).await?;
                if let Some(token) = response.next_token() {
                    self.request = self.request.continuation(token);
                } else {
                    self.finished = true;
                }
                Ok(Some(response))
            }
        }
    };
}

search_paginator!(
    SubjectSearchPaginator,
    SubjectSearchRequest,
    Subject,
    search_subjects
);
search_paginator!(
    ResourceSearchPaginator,
    ResourceSearchRequest,
    Resource,
    search_resources
);
search_paginator!(
    ActionSearchPaginator,
    ActionSearchRequest,
    Action,
    search_actions
);

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

    /// Starts stateful Subject Search pagination.
    ///
    /// Continuation requests preserve the validated initial request and
    /// replace only its opaque page token.
    pub fn paginate_subjects(
        &self,
        request: SubjectSearchRequest,
    ) -> Result<SubjectSearchPaginator, AuthZenError> {
        request.validate()?;
        Ok(SubjectSearchPaginator::new(self.clone(), request))
    }

    pub async fn search_resources(
        &self,
        request: ResourceSearchRequest,
    ) -> Result<SearchResponse<Resource>, AuthZenError> {
        request.validate()?;
        self.post(ApiEndpoint::ResourceSearch, &request).await
    }

    /// Starts stateful Resource Search pagination.
    ///
    /// Continuation requests preserve the validated initial request and
    /// replace only its opaque page token.
    pub fn paginate_resources(
        &self,
        request: ResourceSearchRequest,
    ) -> Result<ResourceSearchPaginator, AuthZenError> {
        request.validate()?;
        Ok(ResourceSearchPaginator::new(self.clone(), request))
    }

    pub async fn search_actions(
        &self,
        request: ActionSearchRequest,
    ) -> Result<SearchResponse<Action>, AuthZenError> {
        request.validate()?;
        self.post(ApiEndpoint::ActionSearch, &request).await
    }

    /// Starts stateful Action Search pagination.
    ///
    /// Continuation requests preserve the validated initial request and
    /// replace only its opaque page token.
    pub fn paginate_actions(
        &self,
        request: ActionSearchRequest,
    ) -> Result<ActionSearchPaginator, AuthZenError> {
        request.validate()?;
        Ok(ActionSearchPaginator::new(self.clone(), request))
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

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
    };

    use serde_json::{Value, json};

    use super::*;

    static NETWORK_TEST_IN_USE: AtomicBool = AtomicBool::new(false);

    struct NetworkTestGuard;

    impl NetworkTestGuard {
        fn acquire() -> Self {
            while NETWORK_TEST_IN_USE
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                thread::yield_now();
            }
            Self
        }
    }

    impl Drop for NetworkTestGuard {
        fn drop(&mut self) {
            NETWORK_TEST_IN_USE.store(false, Ordering::Release);
        }
    }

    fn test_client(endpoint: Url, kind: ApiEndpoint) -> AuthZenClient {
        AuthZenClient {
            http: Client::new(),
            endpoints: HashMap::from([(kind, endpoint)]),
            headers: HeaderMap::new(),
            timeout: DEFAULT_TIMEOUT,
            max_response_body_bytes: DEFAULT_MAX_BODY,
        }
    }

    fn json_server(
        responses: Vec<Value>,
    ) -> (
        NetworkTestGuard,
        Url,
        mpsc::Receiver<Value>,
        thread::JoinHandle<()>,
    ) {
        let guard = NetworkTestGuard::acquire();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut content_length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line
                        .strip_prefix("content-length:")
                        .or_else(|| line.strip_prefix("Content-Length:"))
                    {
                        content_length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; content_length];
                reader.read_exact(&mut body).unwrap();
                sender.send(serde_json::from_slice(&body).unwrap()).unwrap();

                let body = serde_json::to_vec(&response).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        (
            guard,
            Url::parse(&format!("http://{address}/search")).unwrap(),
            receiver,
            handle,
        )
    }

    #[tokio::test]
    async fn resource_pagination_changes_only_the_token_and_stops_on_empty_token() {
        let (_network, endpoint, requests, server) = json_server(vec![
            json!({
                "page": {"next_token": "next-1", "count": 1},
                "results": [{"type": "document", "id": "1"}]
            }),
            json!({
                "page": {"next_token": "", "count": 1},
                "results": [{"type": "document", "id": "2"}]
            }),
        ]);
        let client = test_client(endpoint, ApiEndpoint::ResourceSearch);
        let request = ResourceSearchRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::query("document"),
        )
        .with_context(serde_json::Map::from_iter([(
            "tenant".into(),
            json!("acme"),
        )]))
        .with_page(
            crate::PageRequest::new()
                .with_limit(25)
                .with_property("sort", "name"),
        );

        let mut paginator = client.paginate_resources(request).unwrap();
        assert_eq!(
            paginator
                .next_page()
                .await
                .unwrap()
                .unwrap()
                .results()
                .len(),
            1
        );
        assert_eq!(
            paginator
                .next_page()
                .await
                .unwrap()
                .unwrap()
                .results()
                .len(),
            1
        );
        assert!(paginator.next_page().await.unwrap().is_none());

        let first = requests.recv().unwrap();
        let second = requests.recv().unwrap();
        let mut expected_second = first.clone();
        expected_second["page"]["token"] = json!("next-1");
        assert_eq!(second, expected_second);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn resource_pagination_stops_when_the_response_has_no_page() {
        let (_network, endpoint, requests, server) = json_server(vec![json!({
            "results": [{"type": "document", "id": "1"}]
        })]);
        let client = test_client(endpoint, ApiEndpoint::ResourceSearch);
        let request = ResourceSearchRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::query("document"),
        );

        let mut paginator = client.paginate_resources(request).unwrap();
        assert!(paginator.next_page().await.unwrap().is_some());
        assert!(paginator.next_page().await.unwrap().is_none());

        server.join().unwrap();
        assert!(requests.recv().is_ok());
        assert!(requests.try_recv().is_err());
    }

    #[tokio::test]
    async fn subject_pagination_preserves_the_query_across_pages() {
        let (_network, endpoint, requests, server) = json_server(vec![
            json!({
                "page": {"next_token": "subjects-2"},
                "results": [{"type": "user", "id": "alice"}]
            }),
            json!({
                "page": {"next_token": ""},
                "results": [{"type": "user", "id": "bob"}]
            }),
        ]);
        let client = test_client(endpoint, ApiEndpoint::SubjectSearch);
        let request = SubjectSearchRequest::new(
            Subject::query("user"),
            Action::new("read"),
            Resource::new("document", "1"),
        )
        .with_page(crate::PageRequest::new().with_limit(10));

        let mut paginator = client.paginate_subjects(request).unwrap();
        assert!(paginator.next_page().await.unwrap().is_some());
        assert!(paginator.next_page().await.unwrap().is_some());
        assert!(paginator.next_page().await.unwrap().is_none());

        let first = requests.recv().unwrap();
        let second = requests.recv().unwrap();
        let mut expected_second = first.clone();
        expected_second["page"]["token"] = json!("subjects-2");
        assert_eq!(second, expected_second);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn action_pagination_preserves_the_query_across_pages() {
        let (_network, endpoint, requests, server) = json_server(vec![
            json!({
                "page": {"next_token": "actions-2"},
                "results": [{"name": "read"}]
            }),
            json!({
                "page": {"next_token": ""},
                "results": [{"name": "write"}]
            }),
        ]);
        let client = test_client(endpoint, ApiEndpoint::ActionSearch);
        let request = ActionSearchRequest::new(
            Subject::new("user", "alice"),
            Resource::new("document", "1"),
        )
        .with_page(
            crate::PageRequest::new()
                .with_limit(10)
                .with_property("order", "ascending"),
        );

        let mut paginator = client.paginate_actions(request).unwrap();
        assert!(paginator.next_page().await.unwrap().is_some());
        assert!(paginator.next_page().await.unwrap().is_some());
        assert!(paginator.next_page().await.unwrap().is_none());

        let first = requests.recv().unwrap();
        let second = requests.recv().unwrap();
        let mut expected_second = first.clone();
        expected_second["page"]["token"] = json!("actions-2");
        assert_eq!(second, expected_second);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn pagination_propagates_a_malformed_page_response() {
        let (_network, endpoint, _requests, server) = json_server(vec![json!({
            "page": {},
            "results": []
        })]);
        let client = test_client(endpoint, ApiEndpoint::ResourceSearch);
        let request = ResourceSearchRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::query("document"),
        );

        let error = client
            .paginate_resources(request)
            .unwrap()
            .next_page()
            .await
            .unwrap_err();
        assert!(matches!(error, AuthZenError::InvalidResponse(_)));
        server.join().unwrap();
    }

    #[test]
    fn default_paths_are_appended_to_tenant_identifier() {
        let base = Url::parse("https://pdp.example.com/tenant-1").unwrap();
        let endpoints = default_endpoints(&base, HashMap::new()).unwrap();
        assert_eq!(
            endpoints[&ApiEndpoint::Evaluation].as_str(),
            "https://pdp.example.com/tenant-1/access/v1/evaluation"
        );
    }

    #[test]
    fn manual_endpoint_is_used_without_discovery() {
        let base = Url::parse("https://pdp.example.com").unwrap();
        let mut overrides = HashMap::new();
        overrides.insert(
            ApiEndpoint::Evaluation,
            "https://other.example.com/evaluate".into(),
        );
        let endpoints = default_endpoints(&base, overrides).unwrap();
        assert_eq!(
            endpoints[&ApiEndpoint::Evaluation].host_str(),
            Some("other.example.com")
        );
    }

    #[tokio::test]
    async fn binding_rejects_plain_http() {
        let error = match AuthZenClient::builder("http://pdp.example.com")
            .build()
            .await
        {
            Ok(_) => panic!("plain HTTP must be rejected"),
            Err(error) => error,
        };
        assert!(matches!(error, AuthZenError::InvalidMetadata(_)));
    }

    #[tokio::test]
    async fn invalid_bearer_token_is_reported_at_build() {
        let error = match AuthZenClient::builder("https://pdp.example.com")
            .bearer_token("bad\nvalue")
            .build()
            .await
        {
            Ok(_) => panic!("invalid bearer token must be rejected"),
            Err(error) => error,
        };
        assert!(matches!(error, AuthZenError::InvalidRequest(_)));
    }
}
