//! HTTP client execution for Kagi API commands.

use std::{fs, io, path::PathBuf};

use reqwest::{Client as HttpClient, StatusCode};
use sec::Secret;
use serde_json::Value;
use thiserror::Error;

use crate::{
    cli::{ApiFormat, Args, Command, ExtractArgs, SearchArgs},
    request::{RequestError, extract_body, search_body},
};

/// Errors raised while executing API requests.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The Kagi API key was not provided.
    #[error(
        "Kagi API key is required; pass --api-key, set $KAGI_API_KEY, or write ~/.config/kagi/api-key"
    )]
    MissingApiKey,

    /// The configured API key file could not be read.
    #[error("failed to read API key from {path}: {source}", path = path.display())]
    ApiKeyFile {
        /// Path that was read for the API key.
        path: PathBuf,

        /// Underlying file read error.
        #[source]
        source: io::Error,
    },

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
        body: String,
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
    pub async fn search(&self, args: &SearchArgs) -> Result<String, ClientError> {
        match self.post("/search", search_body(args)?).await {
            Ok(body) => Ok(body),
            Err(error) if is_empty_markdown_search_not_found(args, &error) => {
                Ok("No results.".to_string())
            }
            Err(error) => Err(error),
        }
    }

    /// Performs an extraction request.
    pub async fn extract(&self, args: &ExtractArgs) -> Result<String, ClientError> {
        self.post("/extract", extract_body(args)?).await
    }

    /// Sends a JSON request to an API path and returns the raw response.
    async fn post(&self, path: &str, body: Value) -> Result<String, ClientError> {
        let response = self
            .http
            .post(self.endpoint(path))
            .bearer_auth(self.api_key.reveal_str())
            .json(&body)
            .send()
            .await
            .map_err(|source| ClientError::Http { source })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|source| ClientError::Http { source })?;

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

/// Detects an empty markdown search response reported as missing.
fn is_empty_markdown_search_not_found(args: &SearchArgs, error: &ClientError) -> bool {
    matches!(args.format.as_ref(), None | Some(ApiFormat::Markdown))
        && matches!(
            error,
            ClientError::Status { status, body }
                if *status == StatusCode::NOT_FOUND && body.trim().is_empty()
        )
}

/// Resolves the API key from arguments, environment, or configuration.
fn api_key(args: &Args) -> Result<Secret<String>, ClientError> {
    if let Some(api_key) = args.api_key.clone() {
        return Ok(api_key);
    }

    let Some(config_dir) = dirs::config_dir() else {
        return Err(ClientError::MissingApiKey);
    };
    let path = config_dir.join("kagi").join("api-key");
    let key = match fs::read_to_string(&path) {
        Ok(key) => key,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(ClientError::MissingApiKey);
        }
        Err(source) => return Err(ClientError::ApiKeyFile { path, source }),
    };

    let key = key.trim_end_matches(['\r', '\n']).to_string();
    if key.is_empty() {
        return Err(ClientError::MissingApiKey);
    }

    Ok(Secret::new(key))
}

/// Executes the requested command.
pub async fn run(args: Args) -> Result<(), ClientError> {
    let client = KagiClient::new(args.base_url.clone(), api_key(&args)?);

    let result = match &args.command {
        Command::Search(search) => client.search(search).await,
        Command::Extract(extract) => client.extract(extract).await,
    };

    match result {
        Ok(body) => {
            println!("{}", body.trim_end_matches(['\r', '\n']));
            Ok(())
        }
        Err(error) => {
            if let ClientError::Status { body, .. } = &error {
                eprintln!("{body}");
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use crate::{
        cli::{ApiFormat, SearchArgs},
        client::{ClientError, is_empty_markdown_search_not_found},
    };

    /// Returns search arguments for client behavior tests.
    fn search_args(format: Option<ApiFormat>) -> SearchArgs {
        SearchArgs {
            query: vec!["nothing".to_string()],
            workflow: None,
            format,
            lens_id: None,
            lens_json: None,
            sites_included: Vec::new(),
            sites_excluded: Vec::new(),
            keywords_included: Vec::new(),
            keywords_excluded: Vec::new(),
            file_type: None,
            time_after: None,
            time_before: None,
            time_relative: None,
            search_region: None,
            timeout: None,
            page: None,
            limit: None,
            region: None,
            after: None,
            before: None,
            extract_count: None,
            extract_timeout: None,
            safe_search: None,
            domains: Vec::new(),
            regexes: Vec::new(),
            personalizations_json: None,
            request_json: None,
        }
    }

    /// Returns a status error with the given status and body.
    fn status_error(status: StatusCode, body: &str) -> ClientError {
        ClientError::Status {
            status,
            body: body.to_string(),
        }
    }

    #[test]
    fn treats_empty_markdown_search_404_as_no_results() {
        assert!(is_empty_markdown_search_not_found(
            &search_args(None),
            &status_error(StatusCode::NOT_FOUND, "")
        ));
        assert!(is_empty_markdown_search_not_found(
            &search_args(Some(ApiFormat::Markdown)),
            &status_error(StatusCode::NOT_FOUND, "\n")
        ));
    }

    #[test]
    fn keeps_other_status_errors() {
        assert!(!is_empty_markdown_search_not_found(
            &search_args(Some(ApiFormat::Json)),
            &status_error(StatusCode::NOT_FOUND, "")
        ));
        assert!(!is_empty_markdown_search_not_found(
            &search_args(None),
            &status_error(StatusCode::BAD_REQUEST, "")
        ));
        assert!(!is_empty_markdown_search_not_found(
            &search_args(None),
            &status_error(StatusCode::NOT_FOUND, "not found")
        ));
    }
}
