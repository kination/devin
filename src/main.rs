mod apfel;
mod ask;
mod chat;
mod cli;
mod diff;
mod error;
mod tui;

use clap::Parser;
use cli::Cli;

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

    let files = collect_files(&args);

    let result = if let Some(query) = &args.query {
        ask::run(query, &files, args.print, args.diff)
    } else {
        tui::run(&files)
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

/// Merge explicit -f flags with globs from .devin-context (unless --no-context).
fn collect_files(args: &Cli) -> Vec<String> {
    let mut files = args.file.clone();

    if args.no_context {
        return files;
    }

    let ctx_path = ".devin-context";
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
            Err(e) => eprintln!("  .devin-context: bad pattern {line:?}: {e}"),
        }
    }

    files
}
