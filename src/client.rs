//! HTTP client execution for Kagi API commands.

use std::io::{self, Write};

use reqwest::{Client as HttpClient, StatusCode};
use sec::Secret;
use serde_json::Value;
use thiserror::Error;

use crate::{
    cli::{Args, Command, ExtractArgs, SearchArgs},
    request::{RequestError, extract_body, search_body},
};

/// Errors raised while executing API requests.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The Kagi API key was not provided.
    #[error("Kagi API key is required; pass --api-key or set $KAGI_API_KEY")]
    MissingApiKey,

    /// Request body construction failed.
    #[error("request body failed: {source}")]
    Request {
        /// Underlying request construction error.
        #[source]
        source: RequestError,
    },

    /// HTTP request execution failed.
    #[error("request failed: {source}")]
    Http {
        /// Underlying HTTP client error.
        #[source]
        source: reqwest::Error,
    },

    /// Kagi returned a non-success status code.
    #[error("Kagi returned HTTP {status}")]
    Status {
        /// HTTP status code returned by Kagi.
        status: StatusCode,

        /// Response body returned by Kagi.
        body: Vec<u8>,
    },

    /// Response output could not be written.
    #[error("failed to write response: {source}")]
    Output {
        /// Underlying output error.
        #[source]
        source: io::Error,
    },
}

impl From<RequestError> for ClientError {
    /// Converts request construction errors into client errors.
    fn from(source: RequestError) -> Self {
        Self::Request { source }
    }
}

/// Client for the Kagi HTTP API.
pub struct KagiClient {
    /// Underlying HTTP client.
    http: HttpClient,

    /// Base URL for the API.
    base_url: String,

    /// Bearer token used for authentication.
    api_key: Secret<String>,
}

impl KagiClient {
    /// Creates a Kagi API client.
    pub fn new(base_url: String, api_key: Secret<String>) -> Self {
        Self {
            http: HttpClient::new(),
            base_url,
            api_key,
        }
    }

    /// Performs a search request.
    pub async fn search(&self, args: &SearchArgs) -> Result<Vec<u8>, ClientError> {
        self.post("/search", search_body(args)?).await
    }

    /// Performs an extraction request.
    pub async fn extract(&self, args: &ExtractArgs) -> Result<Vec<u8>, ClientError> {
        self.post("/extract", extract_body(args)?).await
    }

    /// Sends a JSON request to an API path and returns the raw response.
    async fn post(&self, path: &str, body: Value) -> Result<Vec<u8>, ClientError> {
        let response = self
            .http
            .post(self.endpoint(path))
            .bearer_auth(self.api_key.reveal_str())
            .json(&body)
            .send()
            .await
            .map_err(|source| ClientError::Http { source })?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|source| ClientError::Http { source })?;
        let body = bytes.to_vec();

        if !status.is_success() {
            return Err(ClientError::Status { status, body });
        }

        Ok(body)
    }

    /// Builds an endpoint URL from the base URL and path.
    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }
}

/// Executes the requested command.
pub async fn run(args: Args) -> Result<(), ClientError> {
    let Some(api_key) = args.api_key.as_ref() else {
        return Err(ClientError::MissingApiKey);
    };
    let client = KagiClient::new(args.base_url.clone(), api_key.clone());

    let result = match &args.command {
        Command::Search(search) => client.search(search).await,
        Command::Extract(extract) => client.extract(extract).await,
    };

    match result {
        Ok(body) => write_stdout(&body),
        Err(error) => {
            if let ClientError::Status { body, .. } = &error {
                write_stderr(body)?;
            }
            Err(error)
        }
    }
}

/// Writes response bytes to standard output.
fn write_stdout(bytes: &[u8]) -> Result<(), ClientError> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout
        .write_all(bytes)
        .and_then(|()| maybe_write_newline(&mut stdout, bytes))
        .map_err(|source| ClientError::Output { source })
}

/// Writes error response bytes to standard error.
fn write_stderr(bytes: &[u8]) -> Result<(), ClientError> {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    stderr
        .write_all(bytes)
        .and_then(|()| maybe_write_newline(&mut stderr, bytes))
        .map_err(|source| ClientError::Output { source })
}

/// Writes a final newline when the response does not already include one.
fn maybe_write_newline(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if !bytes.ends_with(b"\n") {
        writer.write_all(b"\n")?;
    }
    Ok(())
}
