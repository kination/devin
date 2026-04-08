use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use crossterm::QueueableCommand;

use crate::apfel::{Client, Message, build_file_context, ensure_server, model};
use crate::fs_context;
use crate::memory::MemoryStore;
use crate::slow::{SlowSession, UserSignal};
use crate::diff::{compute_diff, parse_blocks, DiffKind};
use crate::error::Result;

const BRAND:   Color = Color::Rgb { r: 138, g: 180, b: 248 };
const MUTED:   Color = Color::Rgb { r: 154, g: 160, b: 166 };
const SUCCESS: Color = Color::Rgb { r: 129, g: 201, b: 149 };
const BORDER:  Color = Color::Rgb { r: 60,  g: 64,  b: 67  };

pub fn run(files: &[String], quick: bool) -> Result<()> {
    let _server = ensure_server()?;
    let client   = Client::new();
    let file_ctx = build_file_context(files);
    let mut slow = SlowSession::new(!quick);
    let mut memory = MemoryStore::load();

    print_header(files)?;

    let mut history: Vec<Message> = Vec::new();
    let mut prompt_history: Vec<String> = Vec::new();
    let mut session = fs_context::SessionContext::new();

    loop {
        print_divider_top()?;
        print_user_prompt()?;

        let input = match read_line(&prompt_history)? {
            Some(s) => s,
            None => break, // Ctrl-C / EOF
        };

        if input.is_empty() {
            print_divider_bottom()?;
            continue;
        }

        prompt_history.push(input.clone());

        match input.as_str() {
            "/exit" | "/quit" => break,
            s if s.starts_with("/run") => {
                print_divider_bottom()?;
                handle_run(s, &mut history);
                continue;
            }
            "/quick" => {
                slow.disable();
                print_divider_bottom()?;
                continue;
            }
            "/done" => {
                slow.advance(UserSignal::Done);
                print_divider_bottom()?;
                continue;
            }
            "/review" => {
                slow.advance(UserSignal::Review);
                print_divider_bottom()?;
                continue;
            }
            _ => {}
        }

        print_divider_bottom()?;

        // Detect paths/names mentioned in the message and attach their content.
        // session.resolve re-injects previously detected files when nothing new is found.
        let detected = fs_context::detect_paths(&input);
        let to_inject = session.resolve(detected);
        let mentioned: Vec<String> = to_inject
            .iter()
            .filter(|p| p.kind == fs_context::PathKind::File)
            .map(|p| p.resolved.to_string_lossy().into_owned())
            .collect();
        let mention_ctx: String = to_inject.iter().map(|p| fs_context::read_path(p)).collect();

        let memory_ctx = memory.build_context();
        let content = {
            let base_ctx = if history.is_empty() {
                match (file_ctx.is_empty(), memory_ctx.is_empty()) {
                    (false, false) => format!("{memory_ctx}\n\n{file_ctx}\n\n{input}"),
                    (false, true)  => format!("{file_ctx}\n\n{input}"),
                    (true,  false) => format!("{memory_ctx}\n\n{input}"),
                    (true,  true)  => input.clone(),
                }
            } else {
                input.clone()
            };
            if mention_ctx.is_empty() {
                base_ctx
            } else {
                format!("{mention_ctx}\n\n{base_ctx}")
            }
        };

        let decision = slow.process_input(&input);
        let call_content = if decision.prefix.is_empty() {
            content.clone()
        } else {
            format!("[{}]\n\n{}", decision.prefix, &content)
        };

        // History stores the original; LLM call uses the prefixed version.
        history.push(Message::user(content));
        let mut call_msgs = history[..history.len() - 1].to_vec();
        call_msgs.push(Message::user(call_content));

        println!();
        print_assistant_label()?;

        // Slow mode buffers the full response so the code gate can strip blocks
        // before display. Quick mode (slow.enabled == false) streams live.
        let response = if slow.enabled {
            let raw = client.stream(&call_msgs, |_token| {})?;
            let filtered = slow.filter_response(&raw);
            let mut out = io::stdout();
            let mut line_start = true;
            for ch in filtered.display.chars() {
                if line_start { write!(out, " ")?; line_start = false; }
                write!(out, "{ch}")?;
                if ch == '\n' { line_start = true; }
            }
            out.flush()?;
            raw
        } else {
            let mut line_start = true;
            client.stream(&call_msgs, |token| {
                let mut out = io::stdout();
                for ch in token.chars() {
                    if line_start {
                        let _ = write!(out, " ");
                        line_start = false;
                    }
                    let _ = write!(out, "{ch}");
                    if ch == '\n' { line_start = true; }
                }
                let _ = out.flush();
            })?
        };

        println!("\n");
        history.push(Message::assistant(&response));

        let display = memory.parse_memory_block(&response);
        let blocks = parse_blocks(&display);
        for block in &blocks {
            // If the block has no detected filename but the user mentioned exactly
            // one file in their prompt, treat that file as the write target.
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

        print_footer(files)?;
    }

    let mut out = io::stdout();
    out.queue(ResetColor)?;
    out.flush()?;
    println!();
    Ok(())
}

// ── line editor ───────────────────────────────────────────────────────────────

/// Read one line with cursor movement and prompt history navigation.
/// Returns None on Ctrl-C / EOF.
fn read_line(prompt_history: &[String]) -> Result<Option<String>> {
    let mut out = io::stdout();
    let mut buf: Vec<char> = Vec::new();
    let mut pos: usize = 0; // cursor position within buf
    let mut nav = HistoryNavigator::new(prompt_history.to_vec());

    enable_raw_mode()?;

    let result = loop {
        let ev = match event::read() {
            Ok(e) => e,
            Err(e) => {
                let _ = disable_raw_mode();
                return Err(e.into());
            }
        };

        match ev {
            Event::Key(key) => match key.code {
                KeyCode::Enter => {
                    break Some(buf.into_iter().collect());
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break None;
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break None;
                }
                KeyCode::Char(c) => {
                    buf.insert(pos, c);
                    pos += 1;
                    // Reprint from insertion point to keep terminal in sync
                    let tail: String = buf[pos..].iter().collect();
                    write!(out, "{c}{tail}")?;
                    if !tail.is_empty() {
                        out.queue(cursor::MoveLeft(tail.len() as u16))?;
                    }
                    out.flush()?;
                }
                KeyCode::Backspace => {
                    if pos > 0 {
                        pos -= 1;
                        buf.remove(pos);
                        out.queue(cursor::MoveLeft(1))?;
                        let tail: String = buf[pos..].iter().collect();
                        write!(out, "{tail} ")?;
                        out.queue(cursor::MoveLeft((tail.len() + 1) as u16))?;
                        out.flush()?;
                    }
                }
                KeyCode::Delete => {
                    if pos < buf.len() {
                        buf.remove(pos);
                        let tail: String = buf[pos..].iter().collect();
                        write!(out, "{tail} ")?;
                        out.queue(cursor::MoveLeft((tail.len() + 1) as u16))?;
                        out.flush()?;
                    }
                }
                KeyCode::Left => {
                    if pos > 0 {
                        pos -= 1;
                        out.queue(cursor::MoveLeft(1))?;
                        out.flush()?;
                    }
                }
                KeyCode::Right => {
                    if pos < buf.len() {
                        pos += 1;
                        out.queue(cursor::MoveRight(1))?;
                        out.flush()?;
                    }
                }
                KeyCode::Home => {
                    if pos > 0 {
                        out.queue(cursor::MoveLeft(pos as u16))?;
                        pos = 0;
                        out.flush()?;
                    }
                }
                KeyCode::End => {
                    let diff = buf.len() - pos;
                    if diff > 0 {
                        out.queue(cursor::MoveRight(diff as u16))?;
                        pos = buf.len();
                        out.flush()?;
                    }
                }
                KeyCode::Up => {
                    let current: String = buf.iter().collect();
                    if let Some(text) = nav.press_up(&current) {
                        replace_buf(&mut out, &mut buf, &mut pos, &text)?;
                    }
                }
                KeyCode::Down => {
                    let text = nav.press_down();
                    replace_buf(&mut out, &mut buf, &mut pos, &text)?;
                }
                _ => {}
            },
            _ => {}
        }
    };

    disable_raw_mode()?;
    writeln!(out)?;
    out.flush()?;

    Ok(result)
}

/// Simple y/N prompt (no line-editor needed).
fn read_confirm(prompt: &str) -> Result<bool> {
    let mut out = io::stdout();
    write!(out, "{prompt}")?;
    out.flush()?;

    enable_raw_mode()?;
    let answer = loop {
        match event::read() {
            Ok(Event::Key(key)) => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => break true,
                KeyCode::Enter
                | KeyCode::Char('n')
                | KeyCode::Char('N')
                | KeyCode::Esc => break false,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break false
                }
                _ => {}
            },
            _ => {}
        }
    };
    disable_raw_mode()?;
    writeln!(out)?;
    out.flush()?;
    Ok(answer)
}

// ── render helpers ────────────────────────────────────────────────────────────

fn print_header(_files: &[String]) -> Result<()> {
    let mut out = io::stdout();
    println!();

    out.queue(SetForegroundColor(SUCCESS))?;
    write!(out, "   ▲")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    writeln!(out, "      Entic CLI v{}", env!("CARGO_PKG_VERSION"))?;

    out.queue(SetForegroundColor(SUCCESS))?;
    writeln!(out, "  ▲▲▲")?;

    out.queue(SetForegroundColor(SUCCESS))?;
    write!(out, " ▲▲▲▲▲")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    write!(out, "    Created by ")?;
    out.queue(SetAttribute(Attribute::Bold))?;
    writeln!(out, "kination")?;
    out.queue(ResetColor)?;

    out.queue(SetForegroundColor(MUTED))?;
    write!(out, "   █")?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::White))?;
    write!(out, "      Plan: ")?;
    out.queue(SetForegroundColor(MUTED))?;
    writeln!(out, "{}", model())?;
    out.queue(ResetColor)?;
    println!();

    let width = term_width().saturating_sub(4);
    out.queue(SetForegroundColor(BORDER))?;
    write!(out, "╭")?;
    write!(out, "{}", "─".repeat(width + 2))?;
    writeln!(out, "╮")?;

    write!(out, "│")?;
    out.queue(ResetColor)?;
    write!(out, " Welcome to Entic CLI. Inspired by 'claude code', 'gemini cli'. Powered by build-in MacOS LLM(wrapped by apfel), and other open models")?;
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
    let shortcut_hint = "? for shortcuts";
    write!(out, "{}", " ".repeat(width.saturating_sub(shortcut_hint.len())))?;
    writeln!(out, "{shortcut_hint}")?;

    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "{}", "─".repeat(width))?;
    out.queue(ResetColor)?;

    let left_info = " /exit to quit";
    let right_info = format!("{} files context", files.len());
    write!(out, "{left_info}")?;
    write!(out, "{}", " ".repeat(width.saturating_sub(left_info.len() + right_info.len())))?;
    writeln!(out, "{right_info}")?;

    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "{}", "▀".repeat(width))?;
    out.queue(ResetColor)?;

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

/// Show diff and ask [y/N] write permission. Skips blocks with no detected filename.
pub(crate) fn prompt_and_write(block: &crate::diff::CodeBlock) -> Result<()> {
    let mut out = io::stdout();

    // Skip blocks with no detected path — don't prompt the user to type one
    let Some(path) = &block.filename else {
        return Ok(());
    };

    let old = std::fs::read_to_string(path).unwrap_or_default();
    let diff = compute_diff(&old, &block.content);
    let has_changes = diff.iter().any(|l| l.kind != DiffKind::Context && l.content != "⋯");

    if !has_changes {
        out.queue(SetForegroundColor(MUTED))?;
        writeln!(out, "\n  {path} — no changes")?;
        out.queue(ResetColor)?;
        out.flush()?;
        return Ok(());
    }

    let width = term_width().saturating_sub(4);
    let label = format!(" {path} ");
    let bar = "─".repeat(width.saturating_sub(label.len() + 2));

    writeln!(out)?;
    out.queue(SetForegroundColor(BORDER))?;
    write!(out, "  ┌{label}")?;
    writeln!(out, "{bar}┐")?;
    out.queue(ResetColor)?;

    for line in &diff {
        match line.kind {
            DiffKind::Add => {
                out.queue(SetForegroundColor(SUCCESS))?;
                writeln!(out, "  │ + {}", line.content)?;
            }
            DiffKind::Remove => {
                out.queue(SetForegroundColor(Color::Rgb { r: 220, g: 80, b: 80 }))?;
                writeln!(out, "  │ - {}", line.content)?;
            }
            DiffKind::Context => {
                out.queue(SetForegroundColor(MUTED))?;
                writeln!(out, "  │   {}", line.content)?;
            }
        }
    }

    out.queue(SetForegroundColor(BORDER))?;
    writeln!(out, "  └{}", "─".repeat(width.saturating_sub(2)))?;
    out.queue(ResetColor)?;

    out.queue(SetForegroundColor(Color::White))?;
    write!(out, "\n  Write to {path}? ")?;
    out.queue(SetForegroundColor(MUTED))?;
    out.flush()?;

    if read_confirm("[y/N] ")? {
        match std::fs::write(path, &block.content) {
            Ok(_) => {
                out.queue(SetForegroundColor(SUCCESS))?;
                write!(out, "  ✓ ")?;
                out.queue(ResetColor)?;
                writeln!(out, "written to {path}")?;
            }
            Err(e) => {
                out.queue(SetForegroundColor(Color::Rgb { r: 220, g: 80, b: 80 }))?;
                write!(out, "  ✗ ")?;
                out.queue(ResetColor)?;
                writeln!(out, "{e}")?;
            }
        }
    } else {
        out.queue(SetForegroundColor(MUTED))?;
        writeln!(out, "  skipped")?;
        out.queue(ResetColor)?;
    }

    writeln!(out)?;
    out.flush()?;
    Ok(())
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

/// Replace the current input buffer with `text`, repainting the terminal line.
fn replace_buf(out: &mut io::Stdout, buf: &mut Vec<char>, pos: &mut usize, text: &str) -> Result<()> {
    if *pos > 0 {
        out.queue(cursor::MoveLeft(*pos as u16))?;
    }
    let old_len = buf.len();
    if old_len > 0 {
        write!(out, "{}", " ".repeat(old_len))?;
        out.queue(cursor::MoveLeft(old_len as u16))?;
    }
    write!(out, "{text}")?;
    *buf = text.chars().collect();
    *pos = buf.len();
    out.flush()?;
    Ok(())
}

// ── history navigator ─────────────────────────────────────────────────────────

struct HistoryNavigator {
    history: Vec<String>,
    idx: Option<usize>, // distance from end: 0 = last entry, 1 = second-to-last
    saved: String,      // in-progress text preserved before navigating up
}

impl HistoryNavigator {
    fn new(history: Vec<String>) -> Self {
        Self { history, idx: None, saved: String::new() }
    }

    /// Move to an older entry. Saves current input on first call.
    /// Returns `Some(text)` or `None` if already at the oldest entry.
    fn press_up(&mut self, current: &str) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }
        let new_idx = match self.idx {
            None => {
                self.saved = current.to_string();
                0
            }
            Some(i) if i + 1 < self.history.len() => i + 1,
            Some(_) => return None,
        };
        self.idx = Some(new_idx);
        Some(self.history[self.history.len() - 1 - new_idx].clone())
    }

    /// Move to a newer entry. Returns saved input when back at live position.
    fn press_down(&mut self) -> String {
        match self.idx {
            None => String::new(),
            Some(0) => {
                self.idx = None;
                self.saved.clone()
            }
            Some(i) => {
                self.idx = Some(i - 1);
                self.history[self.history.len() - i].clone()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_on_empty_history_is_noop() {
        let mut nav = HistoryNavigator::new(vec![]);
        assert_eq!(nav.press_up("typing"), None);
    }

    #[test]
    fn up_returns_most_recent() {
        let mut nav = HistoryNavigator::new(vec!["first".into(), "second".into()]);
        assert_eq!(nav.press_up(""), Some("second".to_string()));
    }

    #[test]
    fn up_twice_goes_older() {
        let mut nav = HistoryNavigator::new(vec!["first".into(), "second".into()]);
        nav.press_up("");
        assert_eq!(nav.press_up(""), Some("first".to_string()));
    }

    #[test]
    fn up_at_oldest_returns_none() {
        let mut nav = HistoryNavigator::new(vec!["only".into()]);
        nav.press_up("");
        assert_eq!(nav.press_up(""), None);
    }

    #[test]
    fn down_after_up_restores_saved() {
        let mut nav = HistoryNavigator::new(vec!["hello".into()]);
        nav.press_up("draft");
        assert_eq!(nav.press_down(), "draft".to_string());
    }

    #[test]
    fn down_when_live_returns_empty() {
        let mut nav = HistoryNavigator::new(vec!["hello".into()]);
        assert_eq!(nav.press_down(), String::new());
    }
}
