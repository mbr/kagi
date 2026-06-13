//! Command-line interface definitions.

use clap::{Parser, Subcommand, ValueEnum};
use sec::Secret;

/// Top-level command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Args {
    /// Kagi API base URL.
    #[arg(long, default_value = "https://kagi.com/api/v1", global = true)]
    pub base_url: String,

    /// Kagi API key literal or `KAGI_API_KEY` environment value.
    #[arg(long, env = "KAGI_API_KEY", hide_env_values = true, global = true)]
    pub api_key: Option<Secret<String>>,

    /// Command to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Kagi API operations.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Perform a Kagi web search.
    Search(Box<SearchArgs>),

    /// Extract page content as markdown from URLs.
    Extract(ExtractArgs),
}

/// Search endpoint arguments.
#[derive(Debug, Parser)]
pub struct SearchArgs {
    /// Search query to run.
    #[arg(required = true, num_args = 1..)]
    pub query: Vec<String>,

    /// Type of results to return.
    #[arg(long, value_enum)]
    pub workflow: Option<Workflow>,

    /// Format to serialize the API response as.
    #[arg(long, value_enum)]
    pub format: Option<ApiFormat>,

    /// Lens to apply to the search.
    #[arg(long = "lens_id")]
    pub lens_id: Option<String>,

    /// Inline lens JSON object to apply to the search.
    #[arg(long = "lens")]
    pub lens_json: Option<String>,

    /// Search only these domains in the inline lens.
    #[arg(long = "lens.sites_included")]
    pub sites_included: Vec<String>,

    /// Exclude these domains in the inline lens.
    #[arg(long = "lens.sites_excluded")]
    pub sites_excluded: Vec<String>,

    /// Return only results containing these keywords in the inline lens.
    #[arg(long = "lens.keywords_included")]
    pub keywords_included: Vec<String>,

    /// Exclude results containing these keywords in the inline lens.
    #[arg(long = "lens.keywords_excluded")]
    pub keywords_excluded: Vec<String>,

    /// File type to search for in the inline lens.
    #[arg(long = "lens.file_type")]
    pub file_type: Option<String>,

    /// Filter for pages updated or published after this date in the inline lens.
    #[arg(long = "lens.time_after")]
    pub time_after: Option<String>,

    /// Filter for pages updated or published before this date in the inline lens.
    #[arg(long = "lens.time_before")]
    pub time_before: Option<String>,

    /// Filter for pages updated or published in a relative interval.
    #[arg(long = "lens.time_relative", value_enum)]
    pub time_relative: Option<TimeRelative>,

    /// Localize results to a region in the inline lens.
    #[arg(long = "lens.search_region")]
    pub search_region: Option<String>,

    /// Number of seconds to allow for collecting search results.
    #[arg(long)]
    pub timeout: Option<f64>,

    /// Page number for paginated results.
    #[arg(long)]
    pub page: Option<u16>,

    /// Maximum number of results to return.
    #[arg(long)]
    pub limit: Option<u16>,

    /// Filter results to a region.
    #[arg(long = "filters.region")]
    pub region: Option<String>,

    /// Filter for results published or updated after this date.
    #[arg(long = "filters.after")]
    pub after: Option<String>,

    /// Filter for results published or updated before this date.
    #[arg(long = "filters.before")]
    pub before: Option<String>,

    /// Number of search results to extract content from.
    #[arg(long = "extract.count")]
    pub extract_count: Option<u8>,

    /// Timeout in seconds for extraction of each search result page.
    #[arg(long = "extract.timeout")]
    pub extract_timeout: Option<f64>,

    /// Whether safe search is enabled.
    #[arg(long = "safe_search")]
    pub safe_search: Option<bool>,

    /// Domain personalization rules as `domain=kind`.
    #[arg(long = "personalizations.domains")]
    pub domains: Vec<String>,

    /// Regex personalization rules as `regex=replacement`.
    #[arg(long = "personalizations.regexes")]
    pub regexes: Vec<String>,

    /// Raw personalizations JSON object.
    #[arg(long = "personalizations")]
    pub personalizations_json: Option<String>,

    /// Raw request JSON object merged after flags.
    #[arg(long = "request-json")]
    pub request_json: Option<String>,
}

/// Extract endpoint arguments.
#[derive(Debug, Parser)]
pub struct ExtractArgs {
    /// Array of pages to extract content from.
    #[arg(required = true, num_args = 1..=10)]
    pub urls: Vec<String>,

    /// Format to serialize the API response as.
    #[arg(long, value_enum)]
    pub format: Option<ApiFormat>,

    /// Optional timeout in seconds for the extraction operation.
    #[arg(long)]
    pub timeout: Option<f64>,

    /// Raw request JSON object merged after flags.
    #[arg(long = "request-json")]
    pub request_json: Option<String>,
}

/// Search workflow values supported by Kagi.
#[derive(Clone, Debug, ValueEnum)]
pub enum Workflow {
    /// Regular web search.
    Search,

    /// Image search.
    Images,

    /// Video search.
    Videos,

    /// News search.
    News,

    /// Podcast search.
    Podcasts,
}

/// API response formats supported by Kagi.
#[derive(Clone, Debug, ValueEnum)]
pub enum ApiFormat {
    /// JSON response format.
    Json,

    /// Markdown response format.
    Markdown,
}

/// Relative time filters supported by Kagi lenses.
#[derive(Clone, Debug, ValueEnum)]
pub enum TimeRelative {
    /// Updated or published in the last day.
    Day,

    /// Updated or published in the last week.
    Week,

    /// Updated or published in the last month.
    Month,
}

impl Workflow {
    /// Returns the API string for the workflow.
    pub fn as_api_value(&self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Images => "images",
            Self::Videos => "videos",
            Self::News => "news",
            Self::Podcasts => "podcasts",
        }
    }
}

impl ApiFormat {
    /// Returns the API string for the response format.
    pub fn as_api_value(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "markdown",
        }
    }
}

impl TimeRelative {
    /// Returns the API string for the relative time filter.
    pub fn as_api_value(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}
