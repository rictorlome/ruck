use std::path::PathBuf;

use clap::{AppSettings, Parser, Subcommand};

/// A fictional versioning CLI
#[derive(Parser)]
#[clap(name = "ruck")]
#[clap(about = "Croc in rust", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[clap(setting(AppSettings::ArgRequiredElseHelp))]
    Send {
        #[clap(required = true, parse(from_os_str))]
        paths: Vec<PathBuf>,
        password: String,
    },
    #[clap(setting(AppSettings::ArgRequiredElseHelp))]
    Receive {
        password: String,
    },
    Relay {},
}
