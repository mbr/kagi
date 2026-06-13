//! Command-line entry point for the Kagi API client.

mod cli;
mod client;
mod request;

use anyhow::Result;

/// Runs the command-line application.
#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::parse_args();
    client::run(args).await?;
    Ok(())
}
