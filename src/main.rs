//! Command-line entry point for the Kagi API client.

mod assistant;
mod cli;
mod client;
mod request;
mod source;

use anyhow::Result;
use clap::Parser;

/// Runs the command-line application.
#[tokio::main]
async fn main() -> Result<()> {
    client::run(cli::Args::parse()).await?;
    Ok(())
}
