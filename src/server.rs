//! Standalone HTTP transport for the Perplexity-compatible search adapter.

use std::{future::IntoFuture, io, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use thiserror::Error;
use tokio::{net::TcpListener, sync::Semaphore};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::{
    cli::ServeArgs,
    client::{ClientError, KagiClient},
    perplexity::{
        KagiData, SearchPlan, SearchRequest, SearchResponse, SearchResult, merge_results,
    },
    response::KagiResponse,
};

/// Maximum simultaneous searches, including multi-query batches.
const MAX_CONCURRENT_SEARCHES: usize = 16;

/// Deadline for an entire search batch, including upstream response bodies.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Shared upstream client and capacity guard.
struct AppState {
    /// Authenticated client whose key never comes from inbound headers.
    client: KagiClient,

    /// Bounds the number of billable requests in flight.
    capacity: Semaphore,
}

/// Failures starting or running the HTTP listener.
#[derive(Debug, Error)]
pub enum ServerError {
    /// The log filter could not be parsed.
    #[error("invalid log filter")]
    LogFilter(#[source] tracing_subscriber::filter::ParseError),

    /// The logging subscriber could not be installed.
    #[error("failed to initialize logging")]
    Logging(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Signal handling could not be registered.
    #[error("failed to register shutdown signals")]
    Signal(#[source] io::Error),

    /// The TCP listener could not be bound.
    #[error("failed to bind HTTP listener")]
    Bind(#[source] io::Error),

    /// HTTP serving failed.
    #[error("HTTP server failed")]
    Serve(#[source] io::Error),
}

/// JSON error exposed to callers without upstream credentials or bodies.
struct ApiError {
    /// HTTP failure category.
    status: StatusCode,

    /// Safe explanation of the failure.
    message: String,
}

/// Wire representation of an API failure.
#[derive(Serialize)]
struct ErrorBody {
    /// Human-readable failure detail.
    detail: String,
}

impl ApiError {
    /// Creates a sanitized HTTP failure.
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Maps upstream failures without exposing Kagi's response body.
    fn upstream(error: ClientError) -> Self {
        match error {
            ClientError::Status { status, .. } => {
                tracing::warn!(%status, "Kagi rejected an upstream request");
                let (status, message) = match status {
                    StatusCode::TOO_MANY_REQUESTS => (status, "Kagi rate limit exceeded"),
                    StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => (
                        StatusCode::BAD_REQUEST,
                        "Kagi rejected the search parameters",
                    ),
                    _ => (StatusCode::BAD_GATEWAY, "Kagi upstream request failed"),
                };
                Self::new(status, message)
            }
            _ => {
                tracing::warn!("Kagi upstream transport failed");
                Self::new(StatusCode::BAD_GATEWAY, "Kagi upstream request failed")
            }
        }
    }
}

impl IntoResponse for ApiError {
    /// Serializes a failure as JSON with its HTTP status.
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                detail: self.message,
            }),
        )
            .into_response()
    }
}

/// Builds routes without consulting inbound authentication headers.
fn router(client: KagiClient) -> Router {
    Router::new()
        .route("/search", post(search))
        .route("/health", get(|| async { StatusCode::OK }))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(Arc::new(AppState {
            client,
            capacity: Semaphore::new(MAX_CONCURRENT_SEARCHES),
        }))
}

/// Validates a request and runs a bounded batch of upstream searches.
#[tracing::instrument(skip_all, fields(request_id))]
async fn search(
    State(state): State<Arc<AppState>>,
    request: Result<Json<SearchRequest>, JsonRejection>,
) -> Result<Json<SearchResponse>, ApiError> {
    let id = Uuid::now_v7().to_string();
    tracing::Span::current().record("request_id", &id);
    let Json(request) =
        request.map_err(|rejection| ApiError::new(rejection.status(), rejection.body_text()))?;
    let plan = request
        .into_plan()
        .map_err(|error| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    let _permit = state
        .capacity
        .try_acquire()
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Search capacity exhausted"))?;
    let results = tokio::time::timeout(SEARCH_TIMEOUT, execute(&state.client, plan))
        .await
        .map_err(|_| {
            tracing::warn!("Search deadline exceeded");
            ApiError::new(StatusCode::GATEWAY_TIMEOUT, "Kagi search timed out")
        })??;
    tracing::info!(results = results.len(), "Search completed");
    Ok(Json(SearchResponse { id, results }))
}

/// Executes validated queries and decodes only the upstream web results.
async fn execute(client: &KagiClient, plan: SearchPlan) -> Result<Vec<SearchResult>, ApiError> {
    let mut batches = Vec::with_capacity(plan.requests.len());
    for request in plan.requests {
        let body = client
            .search_request(&request)
            .await
            .map_err(ApiError::upstream)?;
        let response: KagiResponse<KagiData> = serde_json::from_str(&body).map_err(|_| {
            tracing::warn!("Invalid Kagi response schema");
            ApiError::new(StatusCode::BAD_GATEWAY, "Invalid Kagi response")
        })?;
        if let Some(errors) = response.errors
            && !errors.is_empty()
        {
            for error in errors {
                tracing::warn!(code = error.code, "Kagi returned an API error");
            }
            return Err(ApiError::new(StatusCode::BAD_GATEWAY, "Kagi search failed"));
        }
        let data = response.data.ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "Kagi response is missing search data",
            )
        })?;
        batches.push(request.filter_results(data.search));
    }
    Ok(merge_results(batches, plan.limit))
}

/// Starts the HTTP server and drains active requests on termination signals.
pub async fn run(client: KagiClient, args: &ServeArgs) -> Result<(), ServerError> {
    let filter = EnvFilter::try_new(&args.log_filter).map_err(ServerError::LogFilter)?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .try_init()
        .map_err(ServerError::Logging)?;
    let signal = shutdown_signal().map_err(ServerError::Signal)?;
    let listener = TcpListener::bind(args.listen_address)
        .await
        .map_err(ServerError::Bind)?;
    tracing::info!(address = %listener.local_addr().map_err(ServerError::Bind)?, "Listening without inbound authentication");
    let cancellation = CancellationToken::new();
    let server = axum::serve(listener, router(client))
        .with_graceful_shutdown(cancellation.clone().cancelled_owned())
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result.map_err(ServerError::Serve),
        () = signal => {
            tracing::info!("Draining active searches");
            cancellation.cancel();
            server.await.map_err(ServerError::Serve)
        }
    }
}

/// Registers Unix termination signals before accepting requests.
#[cfg(unix)]
fn shutdown_signal() -> io::Result<impl Future<Output = ()>> {
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {},
            _ = terminate.recv() => {},
        }
    })
}

/// Waits for a console interrupt on non-Unix systems.
#[cfg(not(unix))]
fn shutdown_signal() -> io::Result<impl Future<Output = ()>> {
    Ok(async {
        if tokio::signal::ctrl_c().await.is_err() {
            tracing::warn!("Console signal handler failed; shutting down");
        }
    })
}

#[cfg(test)]
mod tests;
