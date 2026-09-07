//! HTTP client execution for Kagi API commands.

use std::{fs, io, path::PathBuf};

use reqwest::{Client as HttpClient, StatusCode};
use sec::Secret;
use serde_json::Value;
use thiserror::Error;

use crate::{
    assistant::{AssistantError, ChatClient},
    cli::{ApiFormat, Args, AskArgs, Command, ExtractArgs, SearchArgs},
    request::{RequestError, ask_extract_body, extract_body, search_body},
    source::{self, SourceError, Sources},
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

    /// The extracted markdown could not be saved.
    #[error("failed to write extracted markdown to {path}: {source}", path = path.display())]
    SaveSource {
        /// Path that was written to.
        path: PathBuf,

        /// Underlying file write error.
        #[source]
        source: io::Error,
    },

    /// The assistant failed to answer the question.
    #[error("assistant failed: {source}")]
    Assistant {
        /// Underlying assistant failure.
        #[source]
        source: AssistantError,
    },

    /// The extracted pages could not be used as answer sources.
    #[error("invalid answer sources")]
    Sources {
        /// Underlying extraction validation error.
        #[source]
        source: SourceError,
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

    /// Extracts and validates every page requested for an answer.
    pub async fn extract_sources(&self, args: &AskArgs) -> Result<Sources, ClientError> {
        let body = self.post("/extract", ask_extract_body(args)?).await?;
        source::prepare(&body, args.extra_urls.len() + 1)
            .map_err(|source| ClientError::Sources { source })
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

/// Extracts sources, optionally saves them, and asks the independent chat API.
async fn ask(
    client: &KagiClient,
    chat: &ChatClient,
    args: &AskArgs,
) -> Result<String, ClientError> {
    let sources = client.extract_sources(args).await?;
    if let Some(path) = &args.save_source {
        fs::write(path, &sources.markdown).map_err(|source| ClientError::SaveSource {
            path: path.clone(),
            source,
        })?;
    }
    let answer = chat
        .answer(&sources.markdown, &args.question.join(" "))
        .await
        .map_err(|source| ClientError::Assistant { source })?;
    Ok(format!(
        "{}\n\nSources:\n{}",
        answer.trim_end(),
        sources.references
    ))
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
        Command::Ask(args) => {
            let chat =
                ChatClient::from_env().map_err(|source| ClientError::Assistant { source })?;
            ask(&client, &chat, args).await
        }
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
    use std::fs;

    use clap::Parser;
    use reqwest::StatusCode;
    use sec::Secret;
    use serde_json::{Value, json};
    use tempfile::tempdir;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, header, method, path},
    };

    use crate::{
        assistant::ChatClient,
        cli::{ApiFormat, Args, Command, SearchArgs},
        client::{ClientError, KagiClient, ask, is_empty_markdown_search_not_found},
        source::SourceError,
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

    /// Exercises extraction, source saving and the isolated chat request together.
    #[tokio::test]
    async fn answers_from_saved_sources_and_rejects_partial_extraction() {
        let server = MockServer::start().await;
        let dir = tempdir().expect("temporary source directory");
        let source_path = dir.path().join("sources.md");
        let args = Args::try_parse_from([
            "kagi",
            "ask",
            "https://example.com/a",
            "Where",
            "do they disagree?",
            "--url",
            "https://example.com/b",
            "--save-source",
            source_path.to_str().expect("UTF-8 temporary path"),
        ])
        .expect("valid ask arguments");
        let Command::Ask(mut args) = args.command else {
            panic!("expected ask command")
        };
        assert_eq!(args.question.join(" "), "Where do they disagree?");
        let kagi = KagiClient::new(server.uri(), Secret::new("kagi-secret".into()));
        let chat = ChatClient::new(
            &format!("{}/v1/", server.uri()),
            "test-model".into(),
            Secret::new("chat-secret".into()),
        )
        .expect("valid chat configuration");
        let extraction = json!({"data": [
            {"url": "https://example.com/a", "markdown": "Revenue: $42."},
            {"url": "https://example.com/b", "markdown": "Revenue: $43."}
        ]});
        let extraction_mock = Mock::given(method("POST"))
            .and(path("/extract"))
            .and(header("authorization", "Bearer kagi-secret"))
            .and(body_partial_json(json!({"format": "json", "pages": [
                {"url": "https://example.com/a"}, {"url": "https://example.com/b"}
            ]})))
            .respond_with(ResponseTemplate::new(200).set_body_json(&extraction))
            .expect(2)
            .mount_as_scoped(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer chat-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "test", "object": "chat.completion", "created": 0, "model": "test-model",
                "choices": [{"index": 0, "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "[1] says '$42'; [2] says '$43'."}}]
            })))
            .expect(2)
            .mount(&server)
            .await;
        let answer = ask(&kagi, &chat, &args)
            .await
            .expect("answer from complete sources");
        assert_eq!(
            answer,
            "[1] says '$42'; [2] says '$43'.\n\nSources:\n[1] https://example.com/a\n[2] https://example.com/b"
        );
        let saved = fs::read_to_string(&source_path).expect("saved sources");
        assert_eq!(
            saved,
            "# Source [1]\n\nURL: https://example.com/a\n\nRevenue: $42.\n\n# Source [2]\n\nURL: https://example.com/b\n\nRevenue: $43."
        );
        let requests = server.received_requests().await.expect("recorded requests");
        let request: Value = requests[1].body_json().expect("chat request JSON");
        assert_eq!(request["model"], "test-model");
        assert_eq!(request["stream"], false);
        assert_eq!(request["max_completion_tokens"], 8192);
        assert!(request.get("tools").is_none());
        assert_eq!(request["messages"].as_array().expect("messages").len(), 2);
        assert_eq!(request["messages"][0]["role"], "system");
        assert_eq!(request["messages"][1]["role"], "user");
        assert_eq!(
            request["messages"][1]["content"],
            format!("Question:\nWhere do they disagree?\n\nSources:\n{saved}")
        );
        assert!(!String::from_utf8_lossy(&requests[1].body).contains("kagi-secret"));

        args.save_source = None;
        assert_eq!(
            ask(&kagi, &chat, &args)
                .await
                .expect("answer without saving sources"),
            answer
        );
        args.save_source = Some(source_path.clone());

        drop(extraction_mock);
        let mut partial = extraction;
        partial["data"][1] = json!({"url": "https://example.com/b", "error": "timed out"});
        Mock::given(path("/extract"))
            .respond_with(ResponseTemplate::new(200).set_body_json(partial))
            .expect(1)
            .mount(&server)
            .await;
        assert!(matches!(
            ask(&kagi, &chat, &args).await,
            Err(ClientError::Sources {
                source: SourceError::Page { .. }
            })
        ));
        assert_eq!(
            fs::read_to_string(source_path).expect("previous saved sources"),
            saved
        );
    }
}
