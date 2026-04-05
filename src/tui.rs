use std::io::{self, BufRead, Write};

use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal;
use crossterm::QueueableCommand;

use crate::apfel::{Client, Message, build_file_context, ensure_server, model};
use crate::diff::parse_blocks;
use crate::error::Result;


const BRAND:   Color = Color::Rgb { r: 138, g: 180, b: 248 }; // blue
const MUTED:   Color = Color::Rgb { r: 154, g: 160, b: 166 }; // gray
const SUCCESS: Color = Color::Rgb { r: 129, g: 201, b: 149 }; // green
const BORDER:  Color = Color::Rgb { r: 60,  g: 64,  b: 67 };  // dark gray

pub fn run(files: &[String]) -> Result<()> {
    let _server = ensure_server()?;
    let client   = Client::new();
    let file_ctx = build_file_context(files);

    print_header(files)?;

    let mut history: Vec<Message> = Vec::new();
    let stdin = io::stdin();

    loop {
        print_divider_top()?;
        print_user_prompt()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 { break; }
        let input = line.trim().to_string();
        if input.is_empty() { 
            print_divider_bottom()?;
            continue; 
        }

        match input.as_str() {
            "/exit" | "/quit" => break,
            s if s.starts_with("/apply") => { 
                print_divider_bottom()?;
                handle_apply(s, &history); 
                continue; 
            }
            s if s.starts_with("/run")   => { 
                print_divider_bottom()?;
                handle_run(s, &mut history); 
                continue; 
            }
            _ => {}
        }

        print_divider_bottom()?;

        let content = if history.is_empty() && !file_ctx.is_empty() {
            format!("{file_ctx}\n\n{input}")
        } else {
            input.clone()
        };
        history.push(Message::user(content));

        println!();
        print_assistant_label()?;

        // --- streaming response ---
        let mut line_start = true; // track whether we're at the start of a new line
        let response = client.stream(&history, |token| {
            let mut out = io::stdout();
            // prefix every new line with two-space indent
            for ch in token.chars() {
                if line_start {
                    let _ = write!(out, " ");
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

        print_footer(files)?;
    }

    let mut out = io::stdout();
    out.queue(ResetColor)?;
    out.flush()?;
    println!();
    Ok(())
}

// ── render helpers ────────────────────────────────────────────────────────────

fn print_header(_files: &[String]) -> Result<()> {
    let mut out = io::stdout();
    println!();

    out.queue(SetAttribute(Attribute::Bold))?;
    writeln!(out, "devin")?;
    out.queue(ResetColor)?;
    println!();

    // Logo and version info
    out.queue(SetForegroundColor(BRAND))?;
    write!(out, " ▝▜▄")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    writeln!(out, "     Devin CLI v{}", env!("CARGO_PKG_VERSION"))?;
    
    out.queue(SetForegroundColor(BRAND))?;
    writeln!(out, "   ▝▜▄")?;
    
    write!(out, "  ▗▟▀")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    write!(out, "    Signed in as ")?;
    out.queue(SetAttribute(Attribute::Bold))?;
    write!(out, "kination")?;
    out.queue(ResetColor)?;
    writeln!(out, " /auth")?;
    
    out.queue(SetForegroundColor(BRAND))?;
    write!(out, " ▝▀")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    write!(out, "      Plan: ")?;
    out.queue(SetForegroundColor(MUTED))?;
    write!(out, "{} ", model())?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    writeln!(out, "/upgrade")?;
    out.queue(ResetColor)?;
    println!();

    // Notification box (optional, making it match the style)
    let width = term_width().saturating_sub(4);
    out.queue(SetForegroundColor(BORDER))?;
    write!(out, "╭")?;
    write!(out, "{}", "─".repeat(width + 2))?;
    writeln!(out, "╮")?;
    
    write!(out, "│")?;
    out.queue(ResetColor)?;
    write!(out, " Welcome to Devin CLI. Inspired by 'claude code', 'gemini cli'. Powered by build-in MacOS LLM, and other open models")?;
    write!(out, "{}", " ".repeat(width.saturating_sub(60)))?;
    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "│")?;

    write!(out, "╰")?;
    write!(out, "{}", "─".repeat(width + 2))?;
    writeln!(out, "╯")?;
    out.queue(ResetColor)?;
    out.flush()?;

    Ok(())
}

fn print_divider_top() -> Result<()> {
    let mut out = io::stdout();
    let width = term_width();
    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "{}", "▀".repeat(width))?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_divider_bottom() -> Result<()> {
    let mut out = io::stdout();
    let width = term_width();
    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "{}", "▄".repeat(width))?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_user_prompt() -> Result<()> {
    let mut out = io::stdout();
    out.queue(SetForegroundColor(MUTED))?;
    write!(out, " > ")?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_assistant_label() -> Result<()> {
    let mut out = io::stdout();
    out.queue(SetForegroundColor(BRAND))?;
    write!(out, "✦ ")?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_status_box(msg: &str) -> Result<()> {
    let mut out = io::stdout();
    let width = term_width().saturating_sub(4);
    let box_width = width.max(msg.len() + 4);
    
    out.queue(SetForegroundColor(BORDER))?;
    write!(out, "╭")?;
    write!(out, "{}", "─".repeat(box_width + 2))?;
    writeln!(out, "╮")?;
    
    write!(out, "│ ")?;
    out.queue(SetForegroundColor(SUCCESS))?;
    write!(out, "✓ ")?;
    out.queue(ResetColor)?;
    write!(out, " {msg}")?;
    let msg_len = msg.len() + 2;
    if box_width > msg_len {
        write!(out, "{}", " ".repeat(box_width - msg_len))?;
    }
    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, " │")?;
    
    write!(out, "│")?;
    write!(out, "{}", " ".repeat(box_width + 2))?;
    writeln!(out, "│")?;

    write!(out, "╰")?;
    write!(out, "{}", "─".repeat(box_width + 2))?;
    writeln!(out, "╯")?;
    out.queue(ResetColor)?;
    out.flush()?;
    Ok(())
}

fn print_footer(files: &[String]) -> Result<()> {
    let mut out = io::stdout();
    let width = term_width();
    
    println!();
    // Shortcuts hint
    let shortcut_hint = "? for shortcuts";
    write!(out, "{}", " ".repeat(width.saturating_sub(shortcut_hint.len())))?;
    writeln!(out, "{shortcut_hint}")?;
    
    // Divider
    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "{}", "─".repeat(width))?;
    out.queue(ResetColor)?;
    
    // Info line
    let left_info = " /exit to quit";
    let right_info = format!("{} files context", files.len());
    write!(out, "{left_info}")?;
    write!(out, "{}", " ".repeat(width.saturating_sub(left_info.len() + right_info.len())))?;
    writeln!(out, "{right_info}")?;
    
    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "{}", "▀".repeat(width))?;
    out.queue(ResetColor)?;
    
    // Bottom input prompt area
    out.queue(SetForegroundColor(MUTED))?;
    write!(out, " > ")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(MUTED))?;
    write!(out, "  Type your message or @path/to/file")?;
    out.queue(ResetColor)?;
    write!(out, "{}", " ".repeat(width.saturating_sub(42)))?;
    println!();
    
    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "{}", "▄".repeat(width))?;
    out.queue(ResetColor)?;
    
    let cwd = std::env::current_dir().unwrap_or_default();
    let cwd_str = cwd.file_name().and_then(|s| s.to_str()).unwrap_or("workspace");
    writeln!(out, " {cwd_str} ({})", cwd.display())?;
    
    out.flush()?;
    Ok(())
}

fn print_block_actions(blocks: &[crate::diff::CodeBlock]) -> Result<()> {
    let mut out = io::stdout();
    let width = term_width().saturating_sub(4);

    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "  {}", "─".repeat(width))?;
    out.queue(ResetColor)?;

    for (i, block) in blocks.iter().enumerate() {
        let name = block.filename.as_deref().unwrap_or("untitled");
        write!(out, "  ")?;
        out.queue(SetForegroundColor(MUTED))?;
        write!(out, "[")?;
        out.queue(SetForegroundColor(BRAND))?;
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
                let _ = print_status_box(&format!("WriteFile {p}"));
            }
            Err(e) => eprintln!("  error: {e}"),
        },
        None => eprintln!("  no filename — use /apply <n> <path>"),
    }
}

fn handle_run(input: &str, history: &mut Vec<Message>) {
    let cmd = input.strip_prefix("/run").unwrap_or("").trim();
    if cmd.is_empty() { eprintln!("  usage: /run <command>"); return; }

    let _ = print_status_box(&format!("RunShellCommand {cmd}"));

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
