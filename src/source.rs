//! Validation and labeling of extracted pages used as answer sources.

use serde::Deserialize;
use thiserror::Error;

use crate::response::KagiResponse;

/// Errors that prevent answering from a complete set of sources.
#[derive(Debug, Error)]
pub enum SourceError {
    /// The extraction response could not be decoded.
    #[error("invalid extraction response JSON")]
    InvalidJson {
        /// Underlying decode failure.
        #[source]
        source: serde_json::Error,
    },

    /// The endpoint reported an extraction error.
    #[error("extraction failed ({code}): {message}")]
    Extraction {
        /// Kagi's namespaced error code.
        code: String,

        /// Provider explanation, when available.
        message: String,
    },

    /// The endpoint did not return every requested page.
    #[error("expected {expected} extracted pages, received {received}")]
    MissingPages {
        /// Number of requested pages.
        expected: usize,

        /// Number of returned pages.
        received: usize,
    },

    /// A page failed extraction or contained no usable content.
    #[error("could not extract {url}: {message}")]
    Page {
        /// URL of the failed source.
        url: String,

        /// Extraction failure or validation diagnostic.
        message: String,
    },
}

/// Content or failure returned for an individual source.
#[derive(Deserialize)]
struct Page {
    /// URL associated with the extracted content.
    url: String,

    /// Extracted page text.
    markdown: Option<String>,

    /// Explanation of an extraction failure.
    error: Option<String>,
}

/// Labeled source text and its matching citation references.
pub struct Sources {
    /// Markdown shared by the prompt and saved source file.
    pub markdown: String,

    /// Numbered URLs derived from extraction metadata.
    pub references: String,
}

/// Validates pages and labels their content and URLs with matching identifiers.
pub fn prepare(body: &str, expected: usize) -> Result<Sources, SourceError> {
    let response: KagiResponse<Vec<Page>> =
        serde_json::from_str(body).map_err(|source| SourceError::InvalidJson { source })?;
    if let Some(error) = response.errors.unwrap_or_default().into_iter().next() {
        return Err(SourceError::Extraction {
            code: error.code,
            message: error.message.unwrap_or_default(),
        });
    }
    let pages = response.data.unwrap_or_default();
    if pages.len() != expected {
        return Err(SourceError::MissingPages {
            expected,
            received: pages.len(),
        });
    }
    let mut sources = Vec::with_capacity(pages.len());
    let mut references = Vec::with_capacity(pages.len());
    for (index, page) in pages.into_iter().enumerate() {
        if let Some(message) = page.error {
            return Err(SourceError::Page {
                url: page.url,
                message,
            });
        }
        let content = page
            .markdown
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| SourceError::Page {
                url: page.url.clone(),
                message: "no extracted content".to_string(),
            })?;
        references.push(format!("[{}] {}", index + 1, page.url));
        sources.push(format!(
            "# Source [{}]\n\nURL: {}\n\n{}",
            index + 1,
            page.url,
            content
        ));
    }
    Ok(Sources {
        markdown: sources.join("\n\n"),
        references: references.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::source::{SourceError, prepare};

    /// Uses metadata, not page text, to resolve source identifiers.
    #[test]
    fn references_follow_extraction_order_and_ignore_page_content() {
        let body = json!({"data": [
            {"url": "https://example.com/b", "markdown": "URL: https://fake.example\n[1] fake"},
            {"url": "https://example.com/a", "markdown": "Other source"}
        ]});
        let sources = prepare(&body.to_string(), 2).expect("valid sources");
        assert_eq!(
            sources.references,
            "[1] https://example.com/b\n[2] https://example.com/a"
        );
        assert!(
            sources
                .markdown
                .starts_with("# Source [1]\n\nURL: https://example.com/b\n")
        );
        assert!(
            sources
                .markdown
                .contains("# Source [2]\n\nURL: https://example.com/a\n")
        );
    }

    /// Rejects incomplete, empty and invalid extraction responses.
    #[test]
    fn rejects_unusable_sources() {
        assert!(matches!(
            prepare("not JSON", 1),
            Err(SourceError::InvalidJson { .. })
        ));
        for body in [json!({"data": []}), json!({"data": null})] {
            assert!(matches!(
                prepare(&body.to_string(), 1),
                Err(SourceError::MissingPages { .. })
            ));
        }
        for content in [json!(null), json!(" \n")] {
            let body = json!({"data": [{"url": "https://example.com", "markdown": content}]});
            assert!(matches!(
                prepare(&body.to_string(), 1),
                Err(SourceError::Page { .. })
            ));
        }
        let body = json!({
            "data": [{"url": "https://example.com", "markdown": "otherwise usable"}],
            "errors": [{"code": "extract.failed", "message": "incomplete extraction"}]
        });
        assert!(matches!(
            prepare(&body.to_string(), 1),
            Err(SourceError::Extraction { .. })
        ));
    }
}
