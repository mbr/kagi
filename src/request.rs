//! Request body construction for Kagi API endpoints.

use serde_json::{Map, Number, Value, json};
use thiserror::Error;

use crate::cli::{ApiFormat, ExtractArgs, SearchArgs};

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
}

/// Builds the request body for `POST /search`.
pub fn search_body(args: &SearchArgs) -> Result<Value, RequestError> {
    let mut body = Map::new();
    body.insert("query".to_string(), Value::String(args.query.join(" ")));

    insert_string(
        &mut body,
        "workflow",
        args.workflow.as_ref().map(|v| v.as_api_value()),
    );
    insert_format(
        &mut body,
        resolve_format(args.format.as_ref(), args.json, args.markdown),
    );
    insert_string(&mut body, "lens_id", args.lens_id.as_deref());
    insert_number(&mut body, "timeout", args.timeout);
    insert_integer(&mut body, "page", args.page);
    insert_integer(&mut body, "limit", args.limit);

    if let Some(lens) = lens_body(args)? {
        body.insert("lens".to_string(), lens);
    }
    if let Some(filters) = filters_body(args) {
        body.insert("filters".to_string(), filters);
    }
    if let Some(extract) = search_extract_body(args) {
        body.insert("extract".to_string(), extract);
    }
    if args.safe_search {
        body.insert("safe_search".to_string(), Value::Bool(true));
    }
    if args.no_safe_search {
        body.insert("safe_search".to_string(), Value::Bool(false));
    }
    if let Some(personalizations) = personalizations_body(args)? {
        body.insert("personalizations".to_string(), personalizations);
    }
    merge_json_arg(&mut body, "request-json", args.request_json.as_deref())?;

    Ok(Value::Object(body))
}

/// Builds the request body for `POST /extract`.
pub fn extract_body(args: &ExtractArgs) -> Result<Value, RequestError> {
    let mut body = Map::new();
    body.insert(
        "pages".to_string(),
        Value::Array(args.urls.iter().map(|url| json!({ "url": url })).collect()),
    );
    insert_format(
        &mut body,
        resolve_format(args.format.as_ref(), args.json, args.markdown),
    );
    insert_number(&mut body, "timeout", args.timeout);
    merge_json_arg(&mut body, "request-json", args.request_json.as_deref())?;
    Ok(Value::Object(body))
}

/// Resolves mutually exclusive format flags to an API value.
pub fn resolve_format(
    format: Option<&ApiFormat>,
    json: bool,
    markdown: bool,
) -> Option<&'static str> {
    if json {
        return Some("json");
    }
    if markdown {
        return Some("markdown");
    }
    format.map(ApiFormat::as_api_value)
}

/// Builds the inline lens object when lens options are present.
fn lens_body(args: &SearchArgs) -> Result<Option<Value>, RequestError> {
    let mut lens = Map::new();
    insert_string_array(&mut lens, "sites_included", &args.sites_included);
    insert_string_array(&mut lens, "sites_excluded", &args.sites_excluded);
    insert_string_array(&mut lens, "keywords_included", &args.keywords_included);
    insert_string_array(&mut lens, "keywords_excluded", &args.keywords_excluded);
    insert_string(&mut lens, "file_type", args.file_type.as_deref());
    insert_string(&mut lens, "time_after", args.time_after.as_deref());
    insert_string(&mut lens, "time_before", args.time_before.as_deref());
    insert_string(
        &mut lens,
        "time_relative",
        args.time_relative.as_ref().map(|v| v.as_api_value()),
    );
    insert_string(&mut lens, "search_region", args.search_region.as_deref());
    merge_json_arg(&mut lens, "lens-json", args.lens_json.as_deref())?;

    Ok((!lens.is_empty()).then_some(Value::Object(lens)))
}

/// Builds the search filters object when filter options are present.
fn filters_body(args: &SearchArgs) -> Option<Value> {
    let mut filters = Map::new();
    insert_string(&mut filters, "region", args.region.as_deref());
    insert_string(&mut filters, "after", args.after.as_deref());
    insert_string(&mut filters, "before", args.before.as_deref());
    (!filters.is_empty()).then_some(Value::Object(filters))
}

/// Builds the search result extraction object when extraction options are present.
fn search_extract_body(args: &SearchArgs) -> Option<Value> {
    let mut extract = Map::new();
    insert_integer(&mut extract, "count", args.extract_count);
    insert_number(&mut extract, "timeout", args.extract_timeout);
    (!extract.is_empty()).then_some(Value::Object(extract))
}

/// Builds personalizations when personalization options are present.
fn personalizations_body(args: &SearchArgs) -> Result<Option<Value>, RequestError> {
    let mut personalizations = Map::new();

    if !args.domains.is_empty() {
        let domains = args
            .domains
            .iter()
            .map(|assignment| {
                let (domain, kind) = split_assignment("domain", "domain=kind", assignment)?;
                Ok(json!({ "domain": domain, "kind": kind }))
            })
            .collect::<Result<Vec<_>, RequestError>>()?;
        personalizations.insert("domains".to_string(), Value::Array(domains));
    }

    if !args.regexes.is_empty() {
        let regexes = args
            .regexes
            .iter()
            .map(|assignment| {
                let (regex, replacement) =
                    split_assignment("rewrite", "regex=replacement", assignment)?;
                Ok(json!({ "regex": regex, "replacement": replacement }))
            })
            .collect::<Result<Vec<_>, RequestError>>()?;
        personalizations.insert("regexes".to_string(), Value::Array(regexes));
    }

    merge_json_arg(
        &mut personalizations,
        "personalizations-json",
        args.personalizations_json.as_deref(),
    )?;

    Ok((!personalizations.is_empty()).then_some(Value::Object(personalizations)))
}

/// Inserts a string field when a value exists.
fn insert_string(body: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::String(value.to_string()));
    }
}

/// Inserts a string array field when values exist.
fn insert_string_array(body: &mut Map<String, Value>, key: &str, values: &[String]) {
    if !values.is_empty() {
        body.insert(
            key.to_string(),
            Value::Array(
                values
                    .iter()
                    .map(|value| Value::String(value.clone()))
                    .collect(),
            ),
        );
    }
}

/// Inserts an unsigned integer field when a value exists.
fn insert_integer(body: &mut Map<String, Value>, key: &str, value: Option<impl Into<u64>>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::Number(Number::from(value.into())));
    }
}

/// Inserts a floating point field when a value exists.
fn insert_number(body: &mut Map<String, Value>, key: &str, value: Option<f64>) {
    if let Some(value) = value.and_then(Number::from_f64) {
        body.insert(key.to_string(), Value::Number(value));
    }
}

/// Inserts a format field when a format value exists.
fn insert_format(body: &mut Map<String, Value>, format: Option<&str>) {
    insert_string(body, "format", format);
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
    use crate::{
        cli::{SearchArgs, Workflow},
        request::search_body,
    };

    /// Returns search arguments with only the query set.
    fn search_args() -> SearchArgs {
        SearchArgs {
            query: vec!["rust".to_string(), "tokio".to_string()],
            workflow: Some(Workflow::Search),
            format: None,
            json: true,
            markdown: false,
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
            safe_search: false,
            no_safe_search: true,
            domains: vec!["example.com=raise".to_string()],
            regexes: vec!["^https://x=https://y".to_string()],
            personalizations_json: None,
            request_json: None,
        }
    }

    #[test]
    fn builds_search_body_from_flags() {
        let body = search_body(&search_args()).expect("search body should build");
        assert_eq!(body["query"], "rust tokio");
        assert_eq!(body["format"], "json");
        assert_eq!(body["lens"]["sites_included"][0], "docs.rs");
        assert_eq!(body["filters"]["region"], "DE");
        assert_eq!(body["extract"]["count"], 3);
        assert_eq!(body["safe_search"], false);
        assert_eq!(body["personalizations"]["domains"][0]["kind"], "raise");
    }
}
