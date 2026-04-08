mod apfel;
mod ask;
mod chat;
mod cli;
mod diff;
mod error;
mod indexer;
mod manifest;
mod parser;
mod paths;
mod memory;
mod slow;
mod tui;

use std::path::PathBuf;

use clap::Parser;
use paths::{default_db_path, default_manifest_path};
use cli::{Cli, Command};

fn main() {
    let args = Cli::parse();

    // --base / --model override env vars so apfel.rs picks them up transparently.
    // Safety: single-threaded at this point, no concurrent env reads yet.
    unsafe {
        if let Some(base) = &args.base {
            std::env::set_var("APFEL_BASE", base);
        }
        if let Some(model) = &args.model {
            std::env::set_var("APFEL_MODEL", model);
        }
    }

    if let Some(Command::Index { path }) = &args.command {
        run_index(path);
        return;
    }

    let files = collect_files(&args);

    let result = if let Some(query) = &args.query {
        ask::run(query, &files, args.print, args.diff)
    } else {
        tui::run(&files, args.quick)
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run_index(path: &PathBuf) {
    use anchordb::AnchorDB;

    let db_path = default_db_path();
    let manifest_path = default_manifest_path();

    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let db = match AnchorDB::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to open anchordb at {}: {e}", db_path.display());
            std::process::exit(1);
        }
    };

    match indexer::index_project(path, &db, &manifest_path) {
        Ok(stats) => {
            println!(
                "Indexed {} files, {} chunks stored. ({} skipped)",
                stats.files_indexed, stats.chunks_saved, stats.files_skipped
            );
        }
        Err(e) => {
            eprintln!("Indexing failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Merge explicit -f flags with globs from .entic-context (unless --no-context).
fn collect_files(args: &Cli) -> Vec<String> {
    let mut files = args.file.clone();

    if args.no_context {
        return files;
    }

    let ctx_path = ".entic-context";
    let Ok(content) = std::fs::read_to_string(ctx_path) else {
        return files;
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match glob::glob(line) {
            Ok(paths) => {
                for entry in paths.flatten() {
                    if let Some(s) = entry.to_str() {
                        if !files.contains(&s.to_string()) {
                            files.push(s.to_string());
                        }
                    }
                }
            }
            Err(e) => eprintln!("  .entic-context: bad pattern {line:?}: {e}"),
        }
    }

    files
}
