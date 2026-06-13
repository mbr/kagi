//! Command-line interface definition and argument preprocessing.

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

/// Top-level command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Args {
    /// Kagi API base URL.
    #[arg(long, default_value = "https://kagi.com/api/v1", global = true)]
    pub base_url: String,

    /// Environment variable that stores the Kagi API key.
    #[arg(long, default_value = "KAGI_API_KEY", global = true)]
    pub api_key_env: String,

    /// Kagi API key literal.
    #[arg(long, global = true)]
    pub api_key: Option<String>,

    /// Command to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Kagi API operations.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Perform a Kagi web search.
    Search(Box<SearchArgs>),

    /// Extract markdown content from URLs.
    Extract(ExtractArgs),
}

/// Search endpoint arguments.
#[derive(Debug, Parser)]
pub struct SearchArgs {
    /// Search query words.
    #[arg(required = true, num_args = 1..)]
    pub query: Vec<String>,

    /// Search workflow to request.
    #[arg(long, value_enum)]
    pub workflow: Option<Workflow>,

    /// API response format.
    #[arg(long, value_enum, conflicts_with_all = ["json", "markdown"])]
    pub format: Option<ApiFormat>,

    /// Request JSON output.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "markdown")]
    pub json: bool,

    /// Request markdown output.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "json")]
    pub markdown: bool,

    /// Lens identifier or share URL.
    #[arg(long = "lens-id")]
    pub lens_id: Option<String>,

    /// Raw inline lens JSON object.
    #[arg(long = "lens-json")]
    pub lens_json: Option<String>,

    /// Domain to include in the inline lens.
    #[arg(long = "site")]
    pub sites_included: Vec<String>,

    /// Domain to exclude in the inline lens.
    #[arg(long = "exclude-site")]
    pub sites_excluded: Vec<String>,

    /// Keyword to include in the inline lens.
    #[arg(long = "include-keyword")]
    pub keywords_included: Vec<String>,

    /// Keyword to exclude in the inline lens.
    #[arg(long = "exclude-keyword")]
    pub keywords_excluded: Vec<String>,

    /// File type to request in the inline lens.
    #[arg(long = "file-type")]
    pub file_type: Option<String>,

    /// Lens lower publication or update date bound.
    #[arg(long = "time-after")]
    pub time_after: Option<String>,

    /// Lens upper publication or update date bound.
    #[arg(long = "time-before")]
    pub time_before: Option<String>,

    /// Lens relative time bound.
    #[arg(long = "time-relative", value_enum)]
    pub time_relative: Option<TimeRelative>,

    /// Lens search region.
    #[arg(long = "search-region")]
    pub search_region: Option<String>,

    /// Search timeout in seconds.
    #[arg(long)]
    pub timeout: Option<f64>,

    /// Search results page number.
    #[arg(long)]
    pub page: Option<u16>,

    /// Maximum number of results to return.
    #[arg(long)]
    pub limit: Option<u16>,

    /// Result filter region.
    #[arg(long)]
    pub region: Option<String>,

    /// Result filter lower date bound.
    #[arg(long)]
    pub after: Option<String>,

    /// Result filter upper date bound.
    #[arg(long)]
    pub before: Option<String>,

    /// Number of top search results to extract.
    #[arg(long = "extract")]
    pub extract_count: Option<u8>,

    /// Per-page extraction timeout for search results.
    #[arg(long = "extract-timeout")]
    pub extract_timeout: Option<f64>,

    /// Enables safe search.
    #[arg(long = "safe-search", action = ArgAction::SetTrue, conflicts_with = "no_safe_search")]
    pub safe_search: bool,

    /// Disables safe search.
    #[arg(long = "no-safe-search", action = ArgAction::SetTrue, conflicts_with = "safe_search")]
    pub no_safe_search: bool,

    /// Domain personalization as `domain=kind`.
    #[arg(long = "domain")]
    pub domains: Vec<String>,

    /// Regex personalization as `regex=replacement`.
    #[arg(long = "rewrite")]
    pub regexes: Vec<String>,

    /// Raw personalizations JSON object.
    #[arg(long = "personalizations-json")]
    pub personalizations_json: Option<String>,

    /// Raw request JSON object merged after flags.
    #[arg(long = "request-json")]
    pub request_json: Option<String>,
}

/// Extract endpoint arguments.
#[derive(Debug, Parser)]
pub struct ExtractArgs {
    /// HTTPS URLs to extract.
    #[arg(required = true, num_args = 1..=10)]
    pub urls: Vec<String>,

    /// API response format.
    #[arg(long, value_enum, conflicts_with_all = ["json", "markdown"])]
    pub format: Option<ApiFormat>,

    /// Request JSON output.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "markdown")]
    pub json: bool,

    /// Request markdown output.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "json")]
    pub markdown: bool,

    /// Extraction timeout in seconds.
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
