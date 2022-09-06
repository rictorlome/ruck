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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();
    match &args.command {
        Commands::Send { paths, password } => {
            println!("Sending {:?}", paths);
            send(paths, password).await?;
        }
        Commands::Receive { password } => {
            println!("Receiving password {}", password);
            receive(password).await?
        }
        Commands::Relay {} => {
            serve().await?;
        }
    }
    Ok(())
}
