use std::io::{self, Write};

use crate::apfel::{Client, Message, build_file_context, ensure_server, extract_mentioned_files};
use crate::memory::MemoryStore;
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
            let mentioned = extract_mentioned_files(query);
            let blocks = parse_blocks(&response);
            for block in &blocks {
                if block.filename.is_none() && mentioned.len() == 1 {
                    let filled = crate::diff::CodeBlock {
                        filename: Some(mentioned[0].clone()),
                        content: block.content.clone(),
                    };
                    prompt_and_write(&filled)?;
                } else {
                    prompt_and_write(block)?;
                }
            }
        }
    }

    Ok(())
}

fn build_prompt(query: &str, files: &[String]) -> String {
    let memory = MemoryStore::load();
    let memory_ctx = memory.build_context();
    let explicit_ctx = build_file_context(files);

    // Also attach any files the user mentioned by absolute path in the query
    let mentioned = extract_mentioned_files(query);
    let mention_ctx = build_file_context(&mentioned);

    let base = match (explicit_ctx.is_empty(), mention_ctx.is_empty()) {
        (true,  true)  => query.to_string(),
        (false, true)  => format!("{explicit_ctx}\n\n{query}"),
        (true,  false) => format!("{mention_ctx}\n\n{query}"),
        (false, false) => format!("{explicit_ctx}\n\n{mention_ctx}\n\n{query}"),
    };

    if memory_ctx.is_empty() {
        base
    } else {
        format!("{memory_ctx}\n\n{base}")
    }
}
