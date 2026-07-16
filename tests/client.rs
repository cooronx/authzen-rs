#![cfg(feature = "client")]

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use authzen_rs::{
    Action, ActionSearchRequest, AuthZenError, EvaluationRequest, PageRequest, Resource,
    ResourceSearchRequest, Subject, SubjectSearchRequest,
    client::{ApiEndpoint, AuthZenClient},
};
use rcgen::generate_simple_self_signed;
use reqwest::Client;
use rustls::{
    ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
};
use serde_json::{Value, json};
use url::Url;

struct ReceivedRequest {
    path: String,
    body: Value,
}

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

fn json_server(
    responses: Vec<Value>,
) -> (
    NetworkTestGuard,
    Url,
    Client,
    mpsc::Receiver<ReceivedRequest>,
    thread::JoinHandle<()>,
) {
    let guard = NetworkTestGuard::acquire();
    let certified = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let certificate = CertificateDer::from(certified.cert.der().to_vec());
    let private_key = PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der());
    let tls = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], private_key.into())
            .unwrap(),
    );
    let http = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        for response in responses {
            let (stream, _) = listener.accept().unwrap();
            let connection = ServerConnection::new(Arc::clone(&tls)).unwrap();
            let mut stream = StreamOwned::new(connection, stream);
            let mut content_length = 0;
            let path;
            let mut body = Vec::new();
            {
                let mut reader = BufReader::new(&mut stream);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                path = request_line.split_whitespace().nth(1).unwrap().into();
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
                body.resize(content_length, 0);
                reader.read_exact(&mut body).unwrap();
            }
            sender
                .send(ReceivedRequest {
                    path,
                    body: serde_json::from_slice(&body).unwrap(),
                })
                .unwrap();

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
        Url::parse(&format!("https://localhost:{}/search", address.port())).unwrap(),
        http,
        receiver,
        handle,
    )
}

async fn test_client(endpoint: &Url, http: Client, kind: ApiEndpoint) -> AuthZenClient {
    AuthZenClient::builder(format!("https://localhost:{}", endpoint.port().unwrap()))
        .http_client(http)
        .endpoint(kind, endpoint.as_str())
        .build()
        .await
        .unwrap()
}

#[tokio::test]
async fn resource_pagination_changes_only_the_token_and_stops_on_empty_token() {
    let (_network, endpoint, http, requests, server) = json_server(vec![
        json!({
            "page": {"next_token": "next-1", "count": 1},
            "results": [{"type": "document", "id": "1"}]
        }),
        json!({
            "page": {"next_token": "", "count": 1},
            "results": [{"type": "document", "id": "2"}]
        }),
    ]);
    let client = test_client(&endpoint, http, ApiEndpoint::ResourceSearch).await;
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
        PageRequest::new()
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

    let first = requests.recv().unwrap().body;
    let second = requests.recv().unwrap().body;
    let mut expected_second = first.clone();
    expected_second["page"]["token"] = json!("next-1");
    assert_eq!(second, expected_second);
    server.join().unwrap();
}

#[tokio::test]
async fn resource_pagination_stops_when_the_response_has_no_page() {
    let (_network, endpoint, http, requests, server) = json_server(vec![json!({
        "results": [{"type": "document", "id": "1"}]
    })]);
    let client = test_client(&endpoint, http, ApiEndpoint::ResourceSearch).await;
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
    let (_network, endpoint, http, requests, server) = json_server(vec![
        json!({
            "page": {"next_token": "subjects-2"},
            "results": [{"type": "user", "id": "alice"}]
        }),
        json!({
            "page": {"next_token": ""},
            "results": [{"type": "user", "id": "bob"}]
        }),
    ]);
    let client = test_client(&endpoint, http, ApiEndpoint::SubjectSearch).await;
    let request = SubjectSearchRequest::new(
        Subject::query("user"),
        Action::new("read"),
        Resource::new("document", "1"),
    )
    .with_page(PageRequest::new().with_limit(10));

    let mut paginator = client.paginate_subjects(request).unwrap();
    assert!(paginator.next_page().await.unwrap().is_some());
    assert!(paginator.next_page().await.unwrap().is_some());
    assert!(paginator.next_page().await.unwrap().is_none());

    let first = requests.recv().unwrap().body;
    let second = requests.recv().unwrap().body;
    let mut expected_second = first.clone();
    expected_second["page"]["token"] = json!("subjects-2");
    assert_eq!(second, expected_second);
    server.join().unwrap();
}

#[tokio::test]
async fn action_pagination_preserves_the_query_across_pages() {
    let (_network, endpoint, http, requests, server) = json_server(vec![
        json!({
            "page": {"next_token": "actions-2"},
            "results": [{"name": "read"}]
        }),
        json!({
            "page": {"next_token": ""},
            "results": [{"name": "write"}]
        }),
    ]);
    let client = test_client(&endpoint, http, ApiEndpoint::ActionSearch).await;
    let request = ActionSearchRequest::new(
        Subject::new("user", "alice"),
        Resource::new("document", "1"),
    )
    .with_page(
        PageRequest::new()
            .with_limit(10)
            .with_property("order", "ascending"),
    );

    let mut paginator = client.paginate_actions(request).unwrap();
    assert!(paginator.next_page().await.unwrap().is_some());
    assert!(paginator.next_page().await.unwrap().is_some());
    assert!(paginator.next_page().await.unwrap().is_none());

    let first = requests.recv().unwrap().body;
    let second = requests.recv().unwrap().body;
    let mut expected_second = first.clone();
    expected_second["page"]["token"] = json!("actions-2");
    assert_eq!(second, expected_second);
    server.join().unwrap();
}

#[tokio::test]
async fn pagination_propagates_a_malformed_page_response() {
    let (_network, endpoint, http, _requests, server) = json_server(vec![json!({
        "page": {},
        "results": []
    })]);
    let client = test_client(&endpoint, http, ApiEndpoint::ResourceSearch).await;
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

#[tokio::test]
async fn default_paths_are_appended_to_tenant_identifier() {
    let (_network, endpoint, http, requests, server) = json_server(vec![json!({"decision": true})]);
    let client = AuthZenClient::builder(format!(
        "https://localhost:{}/tenant-1",
        endpoint.port().unwrap()
    ))
    .http_client(http)
    .build()
    .await
    .unwrap();

    client
        .evaluate(EvaluationRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::new("document", "1"),
        ))
        .await
        .unwrap();

    assert_eq!(
        requests.recv().unwrap().path,
        "/tenant-1/access/v1/evaluation"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn manual_endpoint_is_used_without_discovery() {
    let (_network, endpoint, http, requests, server) = json_server(vec![json!({"decision": true})]);
    let client = AuthZenClient::builder(format!("https://localhost:{}", endpoint.port().unwrap()))
        .http_client(http)
        .endpoint(ApiEndpoint::Evaluation, endpoint.as_str())
        .build()
        .await
        .unwrap();

    client
        .evaluate(EvaluationRequest::new(
            Subject::new("user", "alice"),
            Action::new("read"),
            Resource::new("document", "1"),
        ))
        .await
        .unwrap();

    assert_eq!(requests.recv().unwrap().path, "/search");
    server.join().unwrap();
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
