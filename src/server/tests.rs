//! HTTP integration tests using local, isolated mock upstream servers.

use std::sync::Arc;

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use reqwest::Client;
use sec::Secret;
use serde_json::{Value, json};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinHandle};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::{AppState, MAX_CONCURRENT_SEARCHES, router, search};
use crate::client::KagiClient;

/// Running test listener with automatic task cleanup.
struct TestServer {
    /// Local base URL assigned by the operating system.
    url: String,

    /// Server task stopped when the fixture is dropped.
    task: JoinHandle<()>,
}

impl TestServer {
    /// Starts a router on an ephemeral loopback port.
    async fn start(router: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let url = format!(
            "http://{}",
            listener.local_addr().expect("listener address")
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve test router");
        });
        Self { url, task }
    }
}

impl Drop for TestServer {
    /// Stops the listener even when assertions fail.
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Server pair and captured upstream calls.
struct Fixture {
    /// Isolated Kagi mock with request recording.
    upstream: MockServer,

    /// Perplexity adapter under test.
    server: TestServer,
}

impl Fixture {
    /// Starts an adapter with a deterministic upstream response.
    async fn new(status: StatusCode, body: String) -> Self {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(status.as_u16()).set_body_string(body))
            .mount(&upstream)
            .await;
        let client = KagiClient::new(upstream.uri(), Secret::new("configured-key".into()));
        let server = TestServer::start(router(client)).await;
        Self { upstream, server }
    }

    /// Returns the recorded JSON bodies after checking upstream authentication.
    async fn requests(&self) -> Vec<Value> {
        self.upstream
            .received_requests()
            .await
            .expect("recorded requests")
            .into_iter()
            .map(|request| {
                assert_eq!(request.headers["authorization"], "Bearer configured-key");
                request.body_json().expect("Kagi request JSON")
            })
            .collect()
    }

    /// Returns the adapter's search endpoint.
    fn search_url(&self) -> String {
        format!("{}/search", self.server.url)
    }
}

/// Exercises LiteLLM's wire shape with and without a dummy bearer token.
#[tokio::test]
async fn serves_search_with_configured_key_and_no_inbound_authentication() {
    let fixture = Fixture::new(
        StatusCode::OK,
        json!({"data": {"search": [
            {"title": "Rust", "url": "https://rust-lang.org", "snippet": "Rust language"},
            {"title": "Docs", "url": "https://docs.rs", "time": "2026-01-01"}
        ]}})
        .to_string(),
    )
    .await;
    let http = Client::new();
    assert_eq!(
        http.get(format!("{}/health", fixture.server.url))
            .send()
            .await
            .expect("health response")
            .status(),
        StatusCode::OK
    );
    assert!(fixture.requests().await.is_empty());
    for authenticated in [false, true] {
        let mut request = http.post(fixture.search_url()).json(&json!({
            "query": ["rust", "documentation"], "max_results": 2,
            "search_domain_filter": ["rust-lang.org", "docs.rs", "-example.org"],
            "country": "US", "max_tokens_per_page": 1024
        }));
        if authenticated {
            request = request.bearer_auth("dummy-not-the-kagi-key");
        }
        let response = request.send().await.expect("search response");
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.expect("Perplexity JSON");
        assert!(body["id"].as_str().is_some_and(|id| !id.is_empty()));
        assert_eq!(body["results"].as_array().expect("results").len(), 2);
        assert_eq!(body["results"][0]["snippet"], "Rust language");
        assert_eq!(body["results"][1]["snippet"], "");
        assert_eq!(body["results"][1]["date"], "2026-01-01");
    }
    let requests = fixture.requests().await;
    assert_eq!(requests.len(), 4);
    for request in &requests {
        assert_eq!(request["format"], "json");
        assert_eq!(request["limit"], 2);
        assert_eq!(request["filters"]["region"], "us");
        assert_eq!(request["lens"]["sites_excluded"][0], "example.org");
        assert!(request.get("max_tokens_per_page").is_none());
        assert!(request.get("extract").is_none());
    }
    assert_eq!(
        requests[0]["query"],
        "rust (site:rust-lang.org OR site:docs.rs) -site:example.org"
    );
    assert_eq!(
        requests[1]["query"],
        "documentation (site:rust-lang.org OR site:docs.rs) -site:example.org"
    );
}

/// Rejects malformed, oversized, or unsupported requests without spending credits.
#[tokio::test]
async fn invalid_requests_never_reach_kagi() {
    let fixture = Fixture::new(StatusCode::OK, "{}".into()).await;
    let http = Client::new();
    for (body, status) in [
        (
            json!({"query": "rust", "search_language_filter": ["en"]}).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"query": ["rust", " "]}).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        ("{".into(), StatusCode::BAD_REQUEST),
        (
            json!({"query": "a".repeat(65536)}).to_string(),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let response = http
            .post(fixture.search_url())
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .expect("validation response");
        assert_eq!(response.status(), status);
        let error: Value = response.json().await.expect("JSON error");
        assert!(error["detail"].is_string());
    }
    assert!(fixture.requests().await.is_empty());
}

/// Distinguishes upstream failures from empty searches without leaking bodies.
#[tokio::test]
async fn sanitizes_upstream_errors_and_handles_empty_results() {
    for (upstream_status, upstream_body, expected_status) in [
        (
            StatusCode::UNAUTHORIZED,
            "sensitive upstream body",
            StatusCode::BAD_GATEWAY,
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            "sensitive upstream body",
            StatusCode::TOO_MANY_REQUESTS,
        ),
        (
            StatusCode::BAD_REQUEST,
            "sensitive upstream body",
            StatusCode::BAD_REQUEST,
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "sensitive upstream body",
            StatusCode::BAD_GATEWAY,
        ),
        (StatusCode::OK, "not JSON", StatusCode::BAD_GATEWAY),
        (StatusCode::OK, "{}", StatusCode::BAD_GATEWAY),
        (
            StatusCode::OK,
            r#"{"data":{},"errors":[{"code":"search.failed"}]}"#,
            StatusCode::BAD_GATEWAY,
        ),
        (
            StatusCode::OK,
            r#"{"data":{"search":[{"title":"missing URL"}]}}"#,
            StatusCode::BAD_GATEWAY,
        ),
        (
            StatusCode::OK,
            r#"{"data":{},"error":[{"code":"search.failed","message":"sensitive upstream body"}]}"#,
            StatusCode::BAD_GATEWAY,
        ),
        (StatusCode::OK, r#"{"data":{}}"#, StatusCode::OK),
        (StatusCode::OK, r#"{"data":{"search":[]}}"#, StatusCode::OK),
    ] {
        let fixture = Fixture::new(upstream_status, upstream_body.into()).await;
        let response = Client::new()
            .post(fixture.search_url())
            .json(&json!({"query": "rust"}))
            .send()
            .await
            .expect("search response");
        assert_eq!(response.status(), expected_status, "{upstream_body}");
        let body = response.text().await.expect("response body");
        assert!(!body.contains("sensitive upstream body"));
        assert!(!body.contains("configured-key"));
        let body: Value = serde_json::from_str(&body).expect("JSON response");
        if expected_status == StatusCode::OK {
            assert_eq!(body["results"], json!([]));
        } else {
            assert!(body["detail"].is_string());
        }
    }
}

/// Cancels a stalled upstream request and releases its concurrency permit.
#[tokio::test(start_paused = true)]
async fn upstream_deadline_releases_capacity() {
    let upstream = TestServer::start(Router::new().route(
        "/search",
        post(|| async { std::future::pending::<StatusCode>().await }),
    ))
    .await;
    let state = Arc::new(AppState {
        client: KagiClient::new(upstream.url.clone(), Secret::new("unused".into())),
        capacity: Semaphore::new(1),
    });
    let request = serde_json::from_value(json!({"query": "rust"})).expect("valid search");
    let error = search(State(state.clone()), Ok(Json(request)))
        .await
        .expect_err("upstream deadline");
    assert_eq!(error.status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(state.capacity.available_permits(), 1);
}

/// Refuses excess concurrent work rather than starting more paid searches.
#[tokio::test]
async fn capacity_exhaustion_returns_service_unavailable() {
    let state = Arc::new(AppState {
        client: KagiClient::new("http://127.0.0.1:1".into(), Secret::new("unused".into())),
        capacity: Semaphore::new(MAX_CONCURRENT_SEARCHES),
    });
    let _permits = state
        .capacity
        .acquire_many(MAX_CONCURRENT_SEARCHES as u32)
        .await
        .expect("reserve all capacity");
    let request = serde_json::from_value(json!({"query": "rust"})).expect("valid search");
    let error = search(State(state.clone()), Ok(Json(request)))
        .await
        .expect_err("capacity exhausted");
    assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
}
