mod apfel;
mod ask;
mod chat;
mod cli;
mod diff;
mod error;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Ask { query, file } => ask::run(&query, &file),
        Commands::Chat { file } => chat::run(&file),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
