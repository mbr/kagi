//! Shared Kagi response envelopes, excluding unstable debugging metadata.

use serde::Deserialize;

/// Structured Kagi response with endpoint-specific data.
#[derive(Deserialize)]
pub struct KagiResponse<T> {
    /// Endpoint data, absent on some failures.
    pub data: Option<T>,

    /// Endpoint-level failures, including errors returned with a success status.
    #[serde(alias = "error")]
    pub errors: Option<Vec<KagiError>>,
}

/// Endpoint-level failure reported by Kagi.
#[derive(Deserialize)]
pub struct KagiError {
    /// Namespaced failure code.
    pub code: String,

    /// Optional human-readable explanation.
    pub message: Option<String>,
}
