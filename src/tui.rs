use std::io::{self, BufRead, Write};

use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal;
use crossterm::QueueableCommand;

use crate::apfel::{Client, Message, build_file_context, ensure_server};
use crate::diff::parse_blocks;
use crate::error::Result;

// claude-code–style palette
const BRAND:   Color = Color::Rgb { r: 208, g: 191, b: 255 }; // lavender
const ACCENT:  Color = Color::Rgb { r: 97,  g: 218, b: 251 }; // sky blue
const MUTED:   Color = Color::Rgb { r: 110, g: 110, b: 128 }; // gray
const SUCCESS: Color = Color::Rgb { r: 80,  g: 200, b: 120 }; // green

pub fn run(files: &[String]) -> Result<()> {
    let _server = ensure_server()?;
    let client   = Client::new();
    let file_ctx = build_file_context(files);

    print_header(files)?;

    let mut history: Vec<Message> = Vec::new();
    let stdin = io::stdin();

    loop {
        print_divider()?;
        print_user_prompt()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 { break; }
        let input = line.trim().to_string();
        if input.is_empty() { continue; }

        match input.as_str() {
            "/exit" | "/quit" => break,
            s if s.starts_with("/apply") => { handle_apply(s, &history); continue; }
            s if s.starts_with("/run")   => { handle_run(s, &mut history); continue; }
            _ => {}
        }

        let content = if history.is_empty() && !file_ctx.is_empty() {
            format!("{file_ctx}\n\n{input}")
        } else {
            input.clone()
        };
        history.push(Message::user(content));

        println!();
        print_devin_label()?;

        // --- streaming response ---
        let mut line_start = true; // track whether we're at the start of a new line
        let response = client.stream(&history, |token| {
            let mut out = io::stdout();
            // prefix every new line with two-space indent
            for ch in token.chars() {
                if line_start {
                    let _ = write!(out, "  ");
                    line_start = false;
                }
                let _ = write!(out, "{ch}");
                if ch == '\n' { line_start = true; }
            }
            let _ = out.flush();
        })?;

        println!("\n");
        history.push(Message::assistant(&response));

        // --- code block action hints ---
        let blocks = parse_blocks(&response);
        if !blocks.is_empty() {
            print_block_actions(&blocks)?;
        }
    }

    let mut out = io::stdout();
    out.queue(ResetColor)?;
    out.flush()?;
    println!();
    Ok(())
}

// ── render helpers ────────────────────────────────────────────────────────────

fn print_header(files: &[String]) -> Result<()> {
    let mut out = io::stdout();
    println!();

    // "  devin" brand label
    out.queue(SetForegroundColor(BRAND))?
       .queue(SetAttribute(Attribute::Bold))?;
    write!(out, "  devin")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(MUTED))?;
    writeln!(out, "  on-device AI coding assistant")?;
    out.queue(ResetColor)?;

    // context files
    if !files.is_empty() {
        out.queue(SetForegroundColor(MUTED))?;
        write!(out, "  context ")?;
        out.queue(SetForegroundColor(ACCENT))?;
        writeln!(out, "{}", files.join(", "))?;
        out.queue(ResetColor)?;
    }

    // hint line
    out.queue(SetForegroundColor(MUTED))?;
    writeln!(out, "  /exit  /apply <n> [path]  /run <cmd>")?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_divider() -> Result<()> {
    let mut out = io::stdout();
    let width = term_width().saturating_sub(2);
    out.queue(SetForegroundColor(Color::Rgb { r: 50, g: 50, b: 60 }))?;
    writeln!(out, "\n  {}", "─".repeat(width))?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_user_prompt() -> Result<()> {
    let mut out = io::stdout();
    out.queue(SetForegroundColor(MUTED))?;
    write!(out, "\n  ")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    out.queue(SetAttribute(Attribute::Bold))?;
    write!(out, "you")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(MUTED))?;
    write!(out, " › ")?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_devin_label() -> Result<()> {
    let mut out = io::stdout();
    out.queue(SetForegroundColor(BRAND))?;
    out.queue(SetAttribute(Attribute::Bold))?;
    write!(out, "  devin")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(MUTED))?;
    writeln!(out, " ›")?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_block_actions(blocks: &[crate::diff::CodeBlock]) -> Result<()> {
    let mut out = io::stdout();
    let width = term_width().saturating_sub(4);

    out.queue(SetForegroundColor(Color::Rgb { r: 50, g: 50, b: 60 }))?;
    writeln!(out, "  {}", "─".repeat(width))?;
    out.queue(ResetColor)?;

    for (i, block) in blocks.iter().enumerate() {
        let name = block.filename.as_deref().unwrap_or("untitled");
        write!(out, "  ")?;
        out.queue(SetForegroundColor(MUTED))?;
        write!(out, "[")?;
        out.queue(SetForegroundColor(ACCENT))?;
        out.queue(SetAttribute(Attribute::Bold))?;
        write!(out, "{}", i + 1)?;
        out.queue(ResetColor)?;
        out.queue(SetForegroundColor(MUTED))?;
        write!(out, "] ")?;
        out.queue(ResetColor)?;
        out.queue(SetForegroundColor(Color::White))?;
        writeln!(out, "{name}")?;
        out.queue(ResetColor)?;
    }

    out.queue(SetForegroundColor(MUTED))?;
    writeln!(out, "\n  /apply <n> [path] to write  ·  /apply <n> to use detected path")?;
    out.queue(ResetColor)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}

// ── command handlers ──────────────────────────────────────────────────────────

fn handle_apply(input: &str, history: &[Message]) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let idx = match parts.get(1).and_then(|s| s.parse::<usize>().ok()).filter(|&n| n > 0) {
        Some(n) => n - 1,
        None => { eprintln!("  usage: /apply <n> [path]"); return; }
    };

    let response = history.iter().rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let blocks = parse_blocks(response);
    let block = match blocks.get(idx) {
        Some(b) => b,
        None => { eprintln!("  block {} not found", idx + 1); return; }
    };

    let path = parts.get(2).map(|s| s.to_string()).or_else(|| block.filename.clone());
    match path {
        Some(p) => match std::fs::write(&p, &block.content) {
            Ok(_) => {
                let mut out = io::stdout();
                let _ = out.queue(SetForegroundColor(SUCCESS));
                let _ = write!(out, "\n  ✓ ");
                let _ = out.queue(ResetColor);
                let _ = writeln!(out, "written to {p}\n");
                let _ = out.flush();
            }
            Err(e) => eprintln!("  error: {e}"),
        },
        None => eprintln!("  no filename — use /apply <n> <path>"),
    }
}

fn handle_run(input: &str, history: &mut Vec<Message>) {
    let cmd = input.strip_prefix("/run").unwrap_or("").trim();
    if cmd.is_empty() { eprintln!("  usage: /run <command>"); return; }

    let mut out = io::stdout();
    let _ = out.queue(SetForegroundColor(MUTED));
    let _ = writeln!(out, "\n  $ {cmd}");
    let _ = out.queue(ResetColor);
    let _ = out.flush();

    match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(output) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            for line in text.lines() {
                println!("  {line}");
            }
            println!();
            history.push(Message::user(
                format!("Command `{cmd}` output:\n```\n{text}\n```")
            ));
        }
        Err(e) => eprintln!("  error: {e}"),
    }
}

fn term_width() -> usize {
    terminal::size().map(|(w, _)| w as usize).unwrap_or(80)
}
