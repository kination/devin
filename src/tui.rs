use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

use crate::apfel::{Client, Message, ensure_server, build_file_context};
use crate::diff::{parse_blocks, CodeBlock};
use crate::error::Result;

struct AppState {
    history: Vec<Message>,
    input: String,
    status: String,
    scroll: u16,
    pending_blocks: Vec<CodeBlock>,
}

pub fn run(files: &[String]) -> Result<()> {
    let _server = ensure_server()?;
    let client = Client::new();

    let file_ctx = build_file_context(files);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState {
        history: Vec::new(),
        input: String::new(),
        status: "Ready".to_string(),
        scroll: 0,
        pending_blocks: Vec::new(),
    };

    if !files.is_empty() {
        state.status = format!("Loaded {} files", files.len());
    }

    let res = run_loop(&mut terminal, &mut state, &client, &file_ctx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    res
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    client: &Client,
    file_ctx: &str,
) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match (key.modifiers, key.code) {
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,
                    (_, KeyCode::Enter) => {
                        let input = state.input.trim().to_string();
                        if !input.is_empty() {
                            if input == "/exit" || input == "/quit" { break; }

                            // Handle local commands
                            if input.starts_with("/apply") {
                                apply_command(state, &input);
                                state.input.clear();
                                continue;
                            }
                            if input.starts_with("/run") {
                                run_command(state, &input);
                                state.input.clear();
                                continue;
                            }

                            state.status = "Thinking...".to_string();
                            terminal.draw(|f| render(f, state))?;

                            let content = if state.history.is_empty() && !file_ctx.is_empty() {
                                format!("{file_ctx}\n\n{input}")
                            } else {
                                input.clone()
                            };

                            state.history.push(Message::user(content));
                            state.input.clear();

                            match client.complete(&state.history) {
                                Ok(response) => {
                                    state.history.push(Message::assistant(&response));
                                    state.pending_blocks = parse_blocks(&response);
                                    state.status = format!("Response complete ({} code blocks)", state.pending_blocks.len());
                                    // Scroll to bottom (approximate)
                                    state.scroll = 1000;
                                }
                                Err(e) => {
                                    state.status = format!("Error: {e}");
                                }
                            }
                        }
                    }
                    (_, KeyCode::Char(c)) => {
                        state.input.push(c);
                    }
                    (_, KeyCode::Backspace) => {
                        state.input.pop();
                    }
                    (_, KeyCode::PageUp) => {
                        state.scroll = state.scroll.saturating_sub(10);
                    }
                    (_, KeyCode::PageDown) => {
                        state.scroll = state.scroll.saturating_add(10);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn apply_command(state: &mut AppState, input: &str) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() < 2 {
        state.status = "Usage: /apply <number> [filename]".to_string();
        return;
    }

    let idx: usize = match parts[1].parse::<usize>() {
        Ok(n) if n > 0 && n <= state.pending_blocks.len() => n - 1,
        _ => {
            state.status = "Invalid number.".to_string();
            return;
        }
    };

    let block = &state.pending_blocks[idx];
    let filename = parts.get(2).map(|s| s.to_string()).or(block.filename.clone());

    match filename {
        Some(f) => {
            match std::fs::write(&f, &block.content) {
                Ok(_) => state.status = format!("File applied successfully: {f}"),
                Err(e) => state.status = format!("Failed to write file: {e}"),
            }
        }
        None => {
            state.status = "Missing filename. Enter /apply <number> <filename>.".to_string();
        }
    }
}

fn run_command(state: &mut AppState, input: &str) {
    let cmd = input.strip_prefix("/run").unwrap().trim();
    if cmd.is_empty() {
        state.status = "Usage: /run <command>".to_string();
        return;
    }

    state.status = format!("Running: {cmd}");
    
    match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            
            let mut result = format!("$ {cmd}\n");
            if !stdout.is_empty() { result.push_str(&stdout); }
            if !stderr.is_empty() { result.push_str(&stderr); }
            
            state.history.push(Message::user(format!("Command output:\n```\n{result}\n```")));
            state.status = format!("Execution complete: {cmd}");
        }
        Err(e) => {
            state.status = format!("Execution failed: {e}");
        }
    }
}

fn render(f: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),     // Messages
            Constraint::Length(3),  // Input
            Constraint::Length(1),  // Status
        ])
        .split(f.area());

    // 1. Messages
    let mut lines = Vec::new();
    for msg in &state.history {
        let (label, color) = match msg.role {
            "user" => ("you", Color::Cyan),
            _ => ("devin", Color::Green),
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{label}: "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]));

        for line in msg.content.lines() {
            // Very basic: hide the file context if it's too long in the first user message
            if msg.role == "user" && line.starts_with("```// ") && lines.len() > 1 {
                // Just show filename comment
                lines.push(Line::from(vec![Span::styled(format!("  [Context: {}]", line), Style::default().fg(Color::DarkGray))]));
                continue;
            }
            if msg.role == "user" && line.starts_with("```") && !line.contains("//") {
                // skip block content in history for brevity if it's the large context
                continue;
            }

            lines.push(Line::from(Span::raw(format!("  {line}"))));
        }
        lines.push(Line::from(""));
    }

    // Add info about pending blocks if any
    if !state.pending_blocks.is_empty() {
        lines.push(Line::from(Span::styled("--- Available Code Blocks ---", Style::default().fg(Color::Yellow))));
        for (i, b) in state.pending_blocks.iter().enumerate() {
            let fname = b.filename.as_deref().unwrap_or("unknown");
            lines.push(Line::from(vec![
                Span::styled(format!("  [{}] ", i + 1), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(format!("File: {fname} (Type ")),
                Span::styled("/apply ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{} to apply)", i + 1)),
            ]));
        }
        lines.push(Line::from(""));
    }

    let messages = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("devin chat"))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll, 0));
    f.render_widget(messages, chunks[0]);

    // 2. Input
    let input = Paragraph::new(state.input.as_str())
        .block(Block::default().borders(Borders::ALL).title("Input (Ctrl+C: Quit, /apply <n>: Apply Code)"));
    f.render_widget(input, chunks[1]);

    // 3. Status
    let status = Paragraph::new(state.status.as_str())
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[2]);
}
