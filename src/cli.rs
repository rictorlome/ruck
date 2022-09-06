use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// A fictional versioning CLI
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
        // Optional password to use, must be 16 characters at least. If none provided, one will be generated.
        #[clap(long, value_parser, required = false)]
        password: Option<String>,
        /// Paths to files to be sent.
        #[clap(value_parser, required = true)]
        paths: Vec<PathBuf>,
    },
    /// Receive file(s). Must provide password shared out of band
    Receive {
        /// Password shared out
        #[clap(value_parser, required = true)]
        password: String,
    },
    /// Start relay server
    Relay {},
}
