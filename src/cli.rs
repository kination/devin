use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "devin",
    about = "On-device AI coding assistant (powered by apfel)",
    version
)]
pub struct Cli {
    /// Single-turn query. Omit to start interactive chat.
    pub query: Option<String>,

    /// Files to attach as context. Repeatable.
    #[arg(short, long, value_name = "FILE")]
    pub file: Vec<String>,

    /// Override backend URL (e.g. http://localhost:11434 for Ollama)
    #[arg(short, long, value_name = "URL")]
    pub base: Option<String>,

    /// Override model name
    #[arg(short, long, value_name = "NAME")]
    pub model: Option<String>,

    /// Show diff and prompt before writing (single-query mode)
    #[arg(long)]
    pub diff: bool,

    /// Raw output only, no formatting — pipe-friendly
    #[arg(short, long)]
    pub print: bool,

    /// Skip .devin-context auto-attach
    #[arg(long)]
    pub no_context: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Index a project directory into anchordb
    Index {
        /// Path to the project to index
        path: PathBuf,
    },
}
