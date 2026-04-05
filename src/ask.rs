use std::io::{self, Write};

use crate::apfel::{Client, Message, build_file_context, ensure_server};
use crate::diff::parse_blocks;
use crate::error::Result;
use crate::tui::prompt_and_write;

/// Single-turn query.
///
/// - `print_only`: skip formatting, plain stdout (pipe-friendly)
/// - `with_diff`:  after response, show diff and prompt write for each code block
pub fn run(query: &str, files: &[String], print_only: bool, with_diff: bool) -> Result<()> {
    let _server = ensure_server()?;
    let client = Client::new();

    let content = build_prompt(query, files);
    let messages = vec![Message::user(content)];

    if print_only {
        let response = client.complete(&messages)?;
        println!("{response}");
    } else {
        let response = client.stream(&messages, |token| {
            print!("{token}");
            let _ = io::stdout().flush();
        })?;
        println!();

        if with_diff {
            let blocks = parse_blocks(&response);
            for block in &blocks {
                prompt_and_write(block)?;
            }
        }
    }

    Ok(())
}

fn build_prompt(query: &str, files: &[String]) -> String {
    let ctx = build_file_context(files);
    if ctx.is_empty() {
        query.to_string()
    } else {
        format!("{ctx}\n\n{query}")
    }
}
