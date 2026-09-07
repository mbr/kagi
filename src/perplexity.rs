//! Translates the supported Perplexity search protocol to and from Kagi.

use std::collections::HashSet;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Search input accepted by LiteLLM's Perplexity provider.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    /// Query text or a batch of related queries.
    query: Query,

    /// Maximum number of results across the response.
    max_results: Option<usize>,

    /// Included domains, or excluded domains prefixed with `-`.
    search_domain_filter: Option<Vec<String>>,

    /// Accepted for compatibility; Kagi snippets have no token budget control.
    max_tokens_per_page: Option<u32>,

    /// Country in which to localize results.
    country: Option<String>,
}

/// Single-query and multi-query wire representations.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Query {
    /// One search query.
    Single(String),

    /// Multiple related queries.
    Multiple(Vec<String>),
}

/// Invalid search parameters that cannot be forwarded to Kagi.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// No queries or too many queries were supplied.
    #[error("query must contain between 1 and 5 queries")]
    QueryCount,

    /// A query contained no text.
    #[error("query must not be empty or whitespace-only")]
    EmptyQuery,

    /// The result limit was outside the web search range.
    #[error("max_results must be between 1 and 20")]
    ResultLimit,

    /// The domain list exceeded the protocol limit.
    #[error("search_domain_filter must contain at most 20 domains")]
    DomainCount,

    /// A filter could not be represented as a Kagi domain lens.
    #[error(
        "search_domain_filter entries must be hostnames, optionally prefixed with '-'; URLs and paths are not supported"
    )]
    Domain,

    /// The country code was not in the expected format.
    #[error("country must be a two-letter ISO 3166-1 alpha-2 code")]
    Country,

    /// The page budget was outside the accepted protocol range.
    #[error("max_tokens_per_page must be between 1 and 1000000")]
    TokenLimit,
}

/// Validated upstream searches and their aggregate result limit.
pub struct SearchPlan {
    /// One billable Kagi request for each incoming query.
    pub requests: Vec<KagiSearchRequest>,

    /// Maximum number of unique results in the response.
    pub limit: usize,
}

/// JSON search request sent to Kagi.
#[derive(Debug, Serialize)]
pub struct KagiSearchRequest {
    /// Search text for this upstream request.
    query: String,

    /// Structured response format required by the adapter.
    format: &'static str,

    /// Web search workflow.
    workflow: &'static str,

    /// Maximum number of results to fetch.
    limit: usize,

    /// Domain constraints applied through an inline lens.
    #[serde(skip_serializing_if = "Option::is_none")]
    lens: Option<Lens>,

    /// Regional search constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    filters: Option<Filters>,
}

/// Domain constraints understood by Kagi.
#[derive(Clone, Debug, Default, Serialize)]
struct Lens {
    /// Domains to search exclusively.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sites_included: Vec<String>,

    /// Domains to exclude.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sites_excluded: Vec<String>,
}

impl Lens {
    /// Adds domain operators because inline lenses are not reliably enforced.
    fn scope_query(&self, mut query: String) -> String {
        let included: Vec<_> = self
            .sites_included
            .iter()
            .map(|domain| format!("site:{domain}"))
            .collect();
        match included.as_slice() {
            [] => {}
            [site] => query.push_str(&format!(" {site}")),
            _ => query.push_str(&format!(" ({})", included.join(" OR "))),
        }
        for domain in &self.sites_excluded {
            query.push_str(&format!(" -site:{domain}"));
        }
        query
    }

    /// Enforces hostname boundaries even if upstream filtering is incomplete.
    fn allows_url(&self, url: &str) -> bool {
        let Ok(url) = Url::parse(url) else {
            return false;
        };
        let Some(host) = url.host_str() else {
            return false;
        };
        let host = host.trim_end_matches('.');
        let matches = |domain: &String| {
            host == domain
                || host
                    .strip_suffix(domain)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        };
        matches!(url.scheme(), "http" | "https")
            && (self.sites_included.is_empty() || self.sites_included.iter().any(matches))
            && !self.sites_excluded.iter().any(matches)
    }
}

impl KagiSearchRequest {
    /// Removes results outside the requested domains before merging rankings.
    pub fn filter_results(&self, results: Vec<KagiResult>) -> Vec<KagiResult> {
        results
            .into_iter()
            .filter(|result| {
                self.lens
                    .as_ref()
                    .is_none_or(|lens| lens.allows_url(&result.url))
            })
            .collect()
    }
}

/// Regional constraints understood by Kagi.
#[derive(Clone, Debug, Serialize)]
struct Filters {
    /// Lowercase country code required by the upstream API.
    region: String,
}

impl SearchRequest {
    /// Validates all parameters before constructing any billable requests.
    pub fn into_plan(self) -> Result<SearchPlan, ValidationError> {
        let queries = match self.query {
            Query::Single(query) => vec![query],
            Query::Multiple(queries) => queries,
        };
        if !(1..=5).contains(&queries.len()) {
            return Err(ValidationError::QueryCount);
        }
        if queries.iter().any(|query| query.trim().is_empty()) {
            return Err(ValidationError::EmptyQuery);
        }
        let limit = self.max_results.unwrap_or(10);
        if !(1..=20).contains(&limit) {
            return Err(ValidationError::ResultLimit);
        }
        if self
            .max_tokens_per_page
            .is_some_and(|tokens| !(1..=1_000_000).contains(&tokens))
        {
            return Err(ValidationError::TokenLimit);
        }
        let filters = self
            .country
            .map(|country| {
                if country.len() != 2 || !country.bytes().all(|byte| byte.is_ascii_alphabetic()) {
                    return Err(ValidationError::Country);
                }
                Ok(Filters {
                    region: country.to_ascii_lowercase(),
                })
            })
            .transpose()?;
        let domains = self.search_domain_filter.unwrap_or_default();
        if domains.len() > 20 {
            return Err(ValidationError::DomainCount);
        }
        let mut lens = Lens::default();
        for entry in domains {
            let (excluded, domain) = match entry.strip_prefix('-') {
                Some(domain) => (true, domain),
                None => (false, entry.as_str()),
            };
            if !valid_domain(domain) {
                return Err(ValidationError::Domain);
            }
            if excluded {
                lens.sites_excluded.push(domain.to_ascii_lowercase());
            } else {
                lens.sites_included.push(domain.to_ascii_lowercase());
            }
        }
        let lens =
            (!lens.sites_included.is_empty() || !lens.sites_excluded.is_empty()).then_some(lens);
        Ok(SearchPlan {
            requests: queries
                .into_iter()
                .map(|query| KagiSearchRequest {
                    query: match &lens {
                        Some(lens) => lens.scope_query(query),
                        None => query,
                    },
                    format: "json",
                    workflow: "search",
                    limit,
                    lens: lens.clone(),
                    filters: filters.clone(),
                })
                .collect(),
            limit,
        })
    }
}

/// Checks that a lens entry is a hostname rather than a URL or search operator.
fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Kagi's web result collection.
#[derive(Deserialize)]
pub struct KagiData {
    /// Web results; other result categories are not part of this adapter.
    #[serde(default)]
    pub search: Vec<KagiResult>,
}

/// Fields needed from a Kagi web result.
#[derive(Deserialize)]
pub struct KagiResult {
    /// Display title.
    title: String,

    /// Destination URL.
    url: String,

    /// Available result excerpt.
    snippet: Option<String>,

    /// Creation or update timestamp reported by Kagi.
    time: Option<String>,
}

/// Perplexity-compatible response envelope.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// Unique request identifier.
    pub id: String,

    /// Ranked web results.
    pub results: Vec<SearchResult>,
}

/// A web result in Perplexity's response schema.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    /// Display title.
    title: String,

    /// Destination URL.
    url: String,

    /// Kagi's excerpt, without additional page extraction.
    snippet: String,

    /// Creation or update timestamp; Kagi does not distinguish the two.
    date: Option<String>,

    /// Separate update date, unavailable from Kagi.
    last_updated: Option<String>,
}

/// Interleaves query rankings, removes duplicate URLs, and caps the total.
pub fn merge_results(batches: Vec<Vec<KagiResult>>, limit: usize) -> Vec<SearchResult> {
    let mut batches: Vec<_> = batches.into_iter().map(Vec::into_iter).collect();
    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for _ in 0..limit {
        for batch in &mut batches {
            if let Some(result) = batch.next()
                && seen.insert(result.url.clone())
            {
                results.push(SearchResult {
                    title: result.title,
                    url: result.url,
                    snippet: result.snippet.unwrap_or_default(),
                    date: result.time,
                    last_updated: None,
                });
                if results.len() == limit {
                    return results;
                }
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{KagiResult, SearchRequest, merge_results};

    /// Validates wire input and exposes the serialized Kagi request for assertions.
    #[test]
    fn translates_litellm_options_and_defaults() {
        let request: SearchRequest = serde_json::from_value(json!({
            "query": ["rust", "tokio"],
            "max_results": 3,
            "search_domain_filter": ["Docs.RS", "-example.com"],
            "country": "DE",
            "max_tokens_per_page": 1024
        }))
        .expect("valid wire request");
        let plan = request.into_plan().expect("valid parameters");
        assert_eq!(plan.limit, 3);
        assert_eq!(plan.requests.len(), 2);
        assert_eq!(
            serde_json::to_value(&plan.requests[0]).expect("serializable request"),
            json!({
                "query": "rust site:docs.rs -site:example.com", "format": "json", "workflow": "search", "limit": 3,
                "lens": {"sites_included": ["docs.rs"], "sites_excluded": ["example.com"]},
                "filters": {"region": "de"}
            })
        );
        let request: SearchRequest = serde_json::from_value(json!({
            "query": "rust", "max_results": null, "country": null,
            "search_domain_filter": [], "max_tokens_per_page": null
        }))
        .expect("nullable options");
        let plan = request.into_plan().expect("default parameters");
        assert_eq!(
            serde_json::to_value(&plan.requests[0]).expect("serializable defaults"),
            json!({"query": "rust", "format": "json", "workflow": "search", "limit": 10})
        );
    }

    /// Rejects invalid or unsupported input before any upstream calls are made.
    #[test]
    fn rejects_invalid_parameters() {
        for input in [
            json!({"query": []}),
            json!({"query": ["a", "b", "c", "d", "e", "f"]}),
            json!({"query": ["valid", " "]}),
            json!({"query": ""}),
            json!({"query": "rust", "max_results": 0}),
            json!({"query": "rust", "max_results": 21}),
            json!({"query": "rust", "max_tokens_per_page": 0}),
            json!({"query": "rust", "max_tokens_per_page": 1000001}),
            json!({"query": "rust", "country": "USA"}),
            json!({"query": "rust", "country": "12"}),
            json!({"query": "rust", "search_domain_filter": ["https://docs.rs/a"]}),
            json!({"query": "rust", "search_domain_filter": ["-"]}),
            json!({"query": "rust", "search_domain_filter": vec!["example.com"; 21]}),
        ] {
            let request: SearchRequest =
                serde_json::from_value(input.clone()).expect("structurally valid request");
            assert!(request.into_plan().is_err(), "accepted {input}");
        }
        for input in [
            json!({"query": "rust", "search_recency_filter": "hour"}),
            json!({"query": "rust", "max_results": "5"}),
            json!({"query": 42}),
            json!({}),
        ] {
            assert!(serde_json::from_value::<SearchRequest>(input).is_err());
        }
    }

    /// Rejects out-of-scope results even when Kagi ignores an inline lens.
    #[test]
    fn domain_filtering_respects_subdomains_and_exclusions() {
        let request: SearchRequest = serde_json::from_value(json!({
            "query": "rust", "search_domain_filter": ["example.com", "docs.rs", "-blocked.example.com"]
        })).expect("valid domain filter");
        let plan = request.into_plan().expect("valid search plan");
        assert_eq!(
            plan.requests[0].query,
            "rust (site:example.com OR site:docs.rs) -site:blocked.example.com"
        );
        for (url, allowed) in [
            ("https://example.com/page", true),
            ("https://WWW.EXAMPLE.COM./page", true),
            ("https://docs.rs", true),
            ("https://blocked.example.com", false),
            ("https://sub.blocked.example.com", false),
            ("https://notexample.com", false),
            ("https://example.com.evil.org", false),
            ("https://example.com@evil.org", false),
            ("https://evil.org/example.com", false),
            ("file://example.com/path", false),
            ("not a URL", false),
        ] {
            let results = vec![
                serde_json::from_value(json!({"title": "Result", "url": url}))
                    .expect("Kagi result"),
            ];
            assert_eq!(
                !plan.requests[0].filter_results(results).is_empty(),
                allowed,
                "{url}"
            );
        }
    }

    /// Preserves snippets and timestamps while fairly merging batch rankings.
    #[test]
    fn merges_and_caps_results_without_inventing_metadata() {
        let batches: Vec<Vec<KagiResult>> = serde_json::from_value(json!([
            [
                {"title": "A", "url": "https://a", "snippet": "Excerpt", "time": "2026-01-02"},
                {"title": "Duplicate", "url": "https://b"},
                {"title": "C", "url": "https://c"}
            ],
            [
                {"title": "B", "url": "https://b", "snippet": null},
                {"title": "D", "url": "https://d"}
            ]
        ]))
        .expect("Kagi results");
        let results =
            serde_json::to_value(merge_results(batches, 3)).expect("serializable response");
        assert_eq!(
            results[0],
            json!({
                "title": "A", "url": "https://a", "snippet": "Excerpt",
                "date": "2026-01-02", "last_updated": null
            })
        );
        assert_eq!(results[1]["url"], "https://b");
        assert_eq!(results[1]["snippet"], "");
        assert_eq!(results[1]["date"], Value::Null);
        assert_eq!(results[2]["url"], "https://d");
        assert_eq!(results.as_array().expect("result array").len(), 3);
        assert!(merge_results(Vec::new(), 10).is_empty());
    }
}
