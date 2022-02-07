mod cli;
mod client;
mod message;
mod server;

use clap::Parser;
use cli::{Cli, Commands};
use client::send;
use server::serve;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();
    match &args.command {
        Commands::Send { paths } => {
            println!("Sending {:?}", paths);
            send(&paths).await?;
        }
        Commands::Receive { password } => {
            println!("Receiving password {}", password);
        }
        Commands::Relay {} => {
            serve().await?;
        }
    }
    Ok(())
}
