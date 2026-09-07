//! Command-line entry point for the Kagi API client.

mod assistant;
mod cli;
mod client;
mod perplexity;
mod request;
mod response;
mod server;
mod source;

use anyhow::Result;
use clap::Parser;

/// Runs the command-line application.
#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Args::parse();
    let client = client::KagiClient::from_args(&args)?;
    let result = match &args.command {
        cli::Command::Serve(serve) => return Ok(server::run(client, serve).await?),
        cli::Command::Search(search) => client.search(search).await,
        cli::Command::Extract(extract) => client.extract(extract).await,
        cli::Command::Ask(ask) => {
            let chat = assistant::ChatClient::from_env()
                .map_err(|source| client::ClientError::Assistant { source })?;
            client::ask(&client, &chat, ask).await
        }
    };
    match result {
        Ok(body) => println!("{}", body.trim_end_matches(['\r', '\n'])),
        Err(error) => {
            if let client::ClientError::Status { body, .. } = &error {
                eprintln!("{body}");
            }
            return Err(error.into());
        }
    }
    Ok(())
}
