//! HTTP client execution for Kagi API commands.

use std::{
    env,
    io::{self, Write},
};

use reqwest::{Client, StatusCode};
use serde_json::Value;
use thiserror::Error;

use crate::{
    cli::{Args, Command},
    request::{RequestError, extract_body, search_body},
};

/// Errors raised while executing API requests.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The configured API key source was empty.
    #[error("Kagi API key is required; pass --api-key or set ${env_var}")]
    MissingApiKey {
        /// Environment variable that was checked for the API key.
        env_var: String,
    },

    /// The configured API key environment variable could not be read.
    #[error("failed to read ${env_var}: {source}")]
    ApiKeyEnv {
        /// Environment variable that was checked for the API key.
        env_var: String,

        /// Underlying environment variable error.
        #[source]
        source: env::VarError,
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

/// Executes the requested command.
pub async fn run(args: Args) -> Result<(), ClientError> {
    let api_key = api_key(&args)?;
    let client = Client::new();

    match args.command {
        Command::Search(search) => {
            post(
                &client,
                &args.base_url,
                "/search",
                &api_key,
                search_body(&search)?,
            )
            .await?;
        }
        Command::Extract(extract) => {
            post(
                &client,
                &args.base_url,
                "/extract",
                &api_key,
                extract_body(&extract)?,
            )
            .await?;
        }
    }

    Ok(())
}

/// Resolves the API key from CLI arguments or the environment.
fn api_key(args: &Args) -> Result<String, ClientError> {
    if let Some(api_key) = args.api_key.as_ref().filter(|value| !value.is_empty()) {
        return Ok(api_key.clone());
    }

    match env::var(&args.api_key_env) {
        Ok(value) if !value.is_empty() => Ok(value),
        Ok(_) | Err(env::VarError::NotPresent) => Err(ClientError::MissingApiKey {
            env_var: args.api_key_env.clone(),
        }),
        Err(source) => Err(ClientError::ApiKeyEnv {
            env_var: args.api_key_env.clone(),
            source,
        }),
    }
}

/// Sends a JSON request to an API path and writes the raw response.
async fn post(
    client: &Client,
    base_url: &str,
    path: &str,
    api_key: &str,
    body: Value,
) -> Result<(), ClientError> {
    let response = client
        .post(endpoint(base_url, path))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|source| ClientError::Http { source })?;

    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|source| ClientError::Http { source })?;

    if !status.is_success() {
        write_stderr(&bytes)?;
        return Err(ClientError::Status { status });
    }

    write_stdout(&bytes)?;
    Ok(())
}

/// Builds an endpoint URL from the base URL and path.
fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
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
