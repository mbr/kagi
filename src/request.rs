//! Request body construction for Kagi API endpoints.

use serde::Serialize;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::cli::{ApiFormat, AskArgs, ExtractArgs, SearchArgs};

/// Maximum number of pages accepted by the extraction endpoint.
const MAX_PAGES: usize = 10;

/// Errors raised while building API requests.
#[derive(Debug, Error)]
pub enum RequestError {
    /// A raw JSON argument could not be parsed.
    #[error("failed to parse {name} as JSON: {source}")]
    InvalidJson {
        /// Name of the JSON argument.
        name: &'static str,

        /// Underlying JSON parse failure.
        #[source]
        source: serde_json::Error,
    },

    /// A raw JSON argument was not an object.
    #[error("{name} must be a JSON object")]
    JsonNotObject {
        /// Name of the JSON argument.
        name: &'static str,
    },

    /// A key-value argument was missing `=`.
    #[error("{name} must use {usage}")]
    InvalidAssignment {
        /// Name of the invalid argument.
        name: &'static str,

        /// Expected assignment form.
        usage: &'static str,
    },

    /// More pages were requested than the endpoint accepts.
    #[error("at most {MAX_PAGES} pages may be extracted at once, got {count}")]
    TooManyPages {
        /// Number of pages that were requested.
        count: usize,
    },

    /// A typed request could not be serialized.
    #[error("failed to serialize {name}: {source}")]
    Serialization {
        /// Name of the request structure.
        name: &'static str,

        /// Underlying JSON serialization failure.
        #[source]
        source: serde_json::Error,
    },
}

/// Request body for `POST /search`.
#[derive(Debug, Serialize)]
struct SearchRequest {
    /// Search query to run.
    query: String,

    /// Type of results to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow: Option<&'static str>,

    /// API response serialization format.
    format: &'static str,

    /// Lens identifier or share URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    lens_id: Option<String>,

    /// Inline lens configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    lens: Option<Value>,

    /// Search collection timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<f64>,

    /// Page number for paginated results.
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u16>,

    /// Maximum number of results to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u16>,

    /// Search result filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    filters: Option<SearchFilters>,

    /// Extraction configuration for top search results.
    #[serde(skip_serializing_if = "Option::is_none")]
    extract: Option<SearchExtraction>,

    /// Safe search setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    safe_search: Option<bool>,

    /// Ranking personalization rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    personalizations: Option<Value>,
}

/// Inline lens configuration for search requests.
#[derive(Debug, Serialize)]
struct Lens {
    /// Domains to search exclusively.
    #[serde(skip_serializing_if = "Option::is_none")]
    sites_included: Option<Vec<String>>,

    /// Domains to exclude from search.
    #[serde(skip_serializing_if = "Option::is_none")]
    sites_excluded: Option<Vec<String>>,

    /// Keywords that results must contain.
    #[serde(skip_serializing_if = "Option::is_none")]
    keywords_included: Option<Vec<String>>,

    /// Keywords that results must not contain.
    #[serde(skip_serializing_if = "Option::is_none")]
    keywords_excluded: Option<Vec<String>>,

    /// File type to search for.
    #[serde(skip_serializing_if = "Option::is_none")]
    file_type: Option<String>,

    /// Lower update or publication date bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    time_after: Option<String>,

    /// Upper update or publication date bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    time_before: Option<String>,

    /// Relative update or publication interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    time_relative: Option<&'static str>,

    /// Search localization region.
    #[serde(skip_serializing_if = "Option::is_none")]
    search_region: Option<String>,
}

/// Search result filters.
#[derive(Debug, Serialize)]
struct SearchFilters {
    /// Region filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>,

    /// Lower date bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    after: Option<String>,

    /// Upper date bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    before: Option<String>,
}

/// Search result extraction settings.
#[derive(Debug, Serialize)]
struct SearchExtraction {
    /// Number of result pages to extract.
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u8>,

    /// Per-page extraction timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<f64>,
}

/// Search personalization rules.
#[derive(Debug, Serialize)]
struct Personalizations {
    /// Domain-level personalization rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    domains: Option<Vec<DomainPersonalization>>,

    /// Regex-based personalization rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    regexes: Option<Vec<RegexPersonalization>>,
}

/// Domain-level personalization rule.
#[derive(Debug, Serialize)]
struct DomainPersonalization {
    /// Domain pattern to personalize.
    domain: String,

    /// Handling mode for the domain pattern.
    kind: String,
}

/// Regex-based personalization rule.
#[derive(Debug, Serialize)]
struct RegexPersonalization {
    /// Pattern to match.
    regex: String,

    /// Replacement to apply.
    replacement: String,
}

/// Request body for `POST /extract`.
#[derive(Debug, Serialize)]
struct ExtractRequest {
    /// Pages to extract.
    pages: Vec<PageInput>,

    /// Extraction timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<f64>,

    /// API response serialization format.
    format: &'static str,
}

/// Page input for extraction requests.
#[derive(Debug, Serialize)]
struct PageInput {
    /// HTTPS URL to extract.
    url: String,
}

/// Builds the request body for `POST /search`.
pub fn search_body(args: &SearchArgs) -> Result<Value, RequestError> {
    let request = SearchRequest {
        query: args.query.join(" "),
        workflow: args.workflow.as_ref().map(|v| v.as_api_value()),
        format: resolve_format(args.format.as_ref()),
        lens_id: args.lens_id.clone(),
        lens: lens_body(args)?,
        timeout: args.timeout,
        page: args.page,
        limit: args.limit,
        filters: filters_body(args),
        extract: search_extract_body(args),
        safe_search: safe_search(args),
        personalizations: personalizations_body(args)?,
    };
    let mut body = serialize_object("search request", &request)?;
    merge_json_arg(&mut body, "request-json", args.request_json.as_deref())?;

    Ok(Value::Object(body))
}

/// Builds the request body for `POST /extract`.
pub fn extract_body(args: &ExtractArgs) -> Result<Value, RequestError> {
    let request = extract_request(
        &args.urls,
        args.timeout,
        resolve_format(args.format.as_ref()),
    )?;
    let mut body = serialize_object("extract request", &request)?;
    merge_json_arg(&mut body, "request-json", args.request_json.as_deref())?;

    Ok(Value::Object(body))
}

/// Builds the extraction request body backing a question.
///
/// JSON preserves per-page errors and URLs before rendering source markdown.
pub fn ask_extract_body(args: &AskArgs) -> Result<Value, RequestError> {
    let request = extract_request(&args.urls(), args.timeout, "json")?;

    Ok(Value::Object(serialize_object(
        "extract request",
        &request,
    )?))
}

/// Builds an extraction request for a set of pages.
fn extract_request(
    urls: &[String],
    timeout: Option<f64>,
    format: &'static str,
) -> Result<ExtractRequest, RequestError> {
    if urls.len() > MAX_PAGES {
        return Err(RequestError::TooManyPages { count: urls.len() });
    }

    Ok(ExtractRequest {
        pages: urls
            .iter()
            .map(|url| PageInput { url: url.clone() })
            .collect(),
        timeout,
        format,
    })
}

/// Resolves the response format to an API value.
pub fn resolve_format(format: Option<&ApiFormat>) -> &'static str {
    format.map_or("markdown", ApiFormat::as_api_value)
}

/// Builds the inline lens object when lens options are present.
fn lens_body(args: &SearchArgs) -> Result<Option<Value>, RequestError> {
    let lens = Lens {
        sites_included: non_empty_strings(&args.sites_included),
        sites_excluded: non_empty_strings(&args.sites_excluded),
        keywords_included: non_empty_strings(&args.keywords_included),
        keywords_excluded: non_empty_strings(&args.keywords_excluded),
        file_type: args.file_type.clone(),
        time_after: args.time_after.clone(),
        time_before: args.time_before.clone(),
        time_relative: args.time_relative.as_ref().map(|v| v.as_api_value()),
        search_region: args.search_region.clone(),
    };
    let mut body = serialize_object("lens", &lens)?;
    merge_json_arg(&mut body, "lens", args.lens_json.as_deref())?;

    Ok((!body.is_empty()).then_some(Value::Object(body)))
}

/// Builds the search filters object when filter options are present.
fn filters_body(args: &SearchArgs) -> Option<SearchFilters> {
    let filters = SearchFilters {
        region: args.region.clone(),
        after: args.after.clone(),
        before: args.before.clone(),
    };
    (filters.region.is_some() || filters.after.is_some() || filters.before.is_some())
        .then_some(filters)
}

/// Builds the search result extraction object when extraction options are present.
fn search_extract_body(args: &SearchArgs) -> Option<SearchExtraction> {
    let extract = SearchExtraction {
        count: args.extract_count,
        timeout: args.extract_timeout,
    };
    (extract.count.is_some() || extract.timeout.is_some()).then_some(extract)
}

/// Builds the safe search setting when one is explicitly supplied.
fn safe_search(args: &SearchArgs) -> Option<bool> {
    args.safe_search
}

/// Builds personalizations when personalization options are present.
fn personalizations_body(args: &SearchArgs) -> Result<Option<Value>, RequestError> {
    let personalizations = Personalizations {
        domains: domain_personalizations(args)?,
        regexes: regex_personalizations(args)?,
    };
    let mut body = serialize_object("personalizations", &personalizations)?;
    merge_json_arg(
        &mut body,
        "personalizations",
        args.personalizations_json.as_deref(),
    )?;

    Ok((!body.is_empty()).then_some(Value::Object(body)))
}

/// Builds domain personalization rules.
fn domain_personalizations(
    args: &SearchArgs,
) -> Result<Option<Vec<DomainPersonalization>>, RequestError> {
    if args.domains.is_empty() {
        return Ok(None);
    }

    args.domains
        .iter()
        .map(|assignment| {
            let (domain, kind) = split_assignment("domain", "domain=kind", assignment)?;
            Ok(DomainPersonalization {
                domain: domain.to_string(),
                kind: kind.to_string(),
            })
        })
        .collect::<Result<Vec<_>, RequestError>>()
        .map(Some)
}

/// Builds regex personalization rules.
fn regex_personalizations(
    args: &SearchArgs,
) -> Result<Option<Vec<RegexPersonalization>>, RequestError> {
    if args.regexes.is_empty() {
        return Ok(None);
    }

    args.regexes
        .iter()
        .map(|assignment| {
            let (regex, replacement) =
                split_assignment("rewrite", "regex=replacement", assignment)?;
            Ok(RegexPersonalization {
                regex: regex.to_string(),
                replacement: replacement.to_string(),
            })
        })
        .collect::<Result<Vec<_>, RequestError>>()
        .map(Some)
}

/// Returns a cloned string vector when it is non-empty.
fn non_empty_strings(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// Serializes a request structure into a JSON object.
fn serialize_object<T: Serialize>(
    name: &'static str,
    value: &T,
) -> Result<Map<String, Value>, RequestError> {
    let value = serde_json::to_value(value)
        .map_err(|source| RequestError::Serialization { name, source })?;
    let Value::Object(value) = value else {
        return Err(RequestError::JsonNotObject { name });
    };
    Ok(value)
}

/// Merges a JSON object argument into an existing object.
fn merge_json_arg(
    target: &mut Map<String, Value>,
    name: &'static str,
    value: Option<&str>,
) -> Result<(), RequestError> {
    let Some(value) = value else {
        return Ok(());
    };
    let parsed: Value =
        serde_json::from_str(value).map_err(|source| RequestError::InvalidJson { name, source })?;
    let Value::Object(parsed) = parsed else {
        return Err(RequestError::JsonNotObject { name });
    };
    target.extend(parsed);
    Ok(())
}

/// Splits a `key=value` argument into both parts.
fn split_assignment<'a>(
    name: &'static str,
    usage: &'static str,
    value: &'a str,
) -> Result<(&'a str, &'a str), RequestError> {
    value
        .split_once('=')
        .ok_or(RequestError::InvalidAssignment { name, usage })
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::{
        cli::{AskArgs, SearchArgs, Workflow},
        request::{RequestError, ask_extract_body, search_body},
    };

    /// Returns search arguments with only the query set.
    fn search_args() -> SearchArgs {
        SearchArgs {
            query: vec!["rust".to_string(), "tokio".to_string()],
            workflow: Some(Workflow::Search),
            format: None,
            lens_id: None,
            lens_json: None,
            sites_included: vec!["docs.rs".to_string()],
            sites_excluded: Vec::new(),
            keywords_included: Vec::new(),
            keywords_excluded: Vec::new(),
            file_type: None,
            time_after: None,
            time_before: None,
            time_relative: None,
            search_region: None,
            timeout: Some(1.0),
            page: Some(2),
            limit: Some(5),
            region: Some("DE".to_string()),
            after: None,
            before: None,
            extract_count: Some(3),
            extract_timeout: Some(2.0),
            safe_search: Some(false),
            domains: vec!["example.com=raise".to_string()],
            regexes: vec!["^https://x=https://y".to_string()],
            personalizations_json: None,
            request_json: None,
        }
    }

    /// Validates the shared page limit and structured extraction for answers.
    #[test]
    fn builds_ask_extraction_with_page_limit() {
        let mut args = AskArgs::try_parse_from(["ask", "https://example.com", "question"])
            .expect("valid ask arguments");
        args.extra_urls = vec!["https://example.com/extra".into(); 9];
        let body = ask_extract_body(&args).expect("ten pages are allowed");
        assert_eq!(body["format"], "json");
        assert_eq!(body["pages"].as_array().expect("page array").len(), 10);
        args.extra_urls.push("https://example.com/overflow".into());
        assert!(matches!(
            ask_extract_body(&args),
            Err(RequestError::TooManyPages { count: 11 })
        ));
        assert!(
            AskArgs::try_parse_from(["ask", "https://example.com", "question", "--model", "test"])
                .is_err()
        );
    }

    #[test]
    fn builds_search_body_from_flags() {
        let body = search_body(&search_args()).expect("search body should build");
        assert_eq!(body["query"], "rust tokio");
        assert_eq!(body["format"], "markdown");
        assert_eq!(body["lens"]["sites_included"][0], "docs.rs");
        assert_eq!(body["filters"]["region"], "DE");
        assert_eq!(body["extract"]["count"], 3);
        assert_eq!(body["safe_search"], false);
        assert_eq!(body["personalizations"]["domains"][0]["kind"], "raise");
    }
}
