use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "devin",
    about = "On-device AI coding assistant (powered by apfel)",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Single question
    Ask {
        /// Question content
        query: String,
        /// Files to attach as context (can be multiple)
        #[arg(short, long, value_name = "FILE")]
        file: Vec<String>,
    },
    /// Interactive chat mode
    Chat {
        /// Files to attach as context (can be multiple)
        #[arg(short, long, value_name = "FILE")]
        file: Vec<String>,
    },
}
