mod cli;
mod client;
mod conf;
mod connection;
mod crypto;
mod file;
mod handshake;
mod message;
mod password;
mod server;
mod ui;

use clap::Parser;
use cli::{Cli, Commands};
use client::{receive, send};
use server::serve;
use std::error::Error;
use tracing::debug;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing with RUST_LOG env filter (defaults to "info")
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("ruck_relay=info".parse().unwrap()))
        .init();

    let args = Cli::parse();
    match &args.command {
        Commands::Send { paths, password, relay } => {
            debug!("Sending {:?}", paths);
            send(paths, password, relay).await?;
        }
        Commands::Receive { password, relay } => {
            debug!("Receiving with provided password");
            receive(password, relay).await?
        }
        Commands::Relay { bind, max_clients, timeout } => {
            serve(bind, *max_clients, *timeout).await?;
        }
    }
    Ok(())
}
