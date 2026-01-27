use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::conf::{DEFAULT_RELAY, DEFAULT_BIND, DEFAULT_MAX_CLIENTS, DEFAULT_PEER_TIMEOUT_SECS};

/// E2E encrypted file transfer via relay
#[derive(Parser, Debug)]
#[clap(name = "ruck")]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Send file(s). Can provide optional password
    Send {
        /// Optional password (if not provided, one will be generated)
        #[clap(long, value_parser, required = false)]
        password: Option<String>,
        /// Relay server address
        #[clap(long, value_parser, default_value = DEFAULT_RELAY)]
        relay: String,
        /// Paths to files to be sent
        #[clap(value_parser, required = true)]
        paths: Vec<PathBuf>,
    },
    /// Receive file(s). Must provide password shared out of band
    Receive {
        /// Password shared by sender
        #[clap(value_parser, required = true)]
        password: String,
        /// Relay server address
        #[clap(long, value_parser, default_value = DEFAULT_RELAY)]
        relay: String,
    },
    /// Start relay server
    Relay {
        /// Address to bind to
        #[clap(long, value_parser, default_value = DEFAULT_BIND)]
        bind: String,
        /// Maximum concurrent pending connections
        #[clap(long, value_parser, default_value_t = DEFAULT_MAX_CLIENTS)]
        max_clients: usize,
        /// Timeout in seconds for peer matching
        #[clap(long, value_parser, default_value_t = DEFAULT_PEER_TIMEOUT_SECS)]
        timeout: u64,
    },
}
