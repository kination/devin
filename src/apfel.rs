use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

use crate::error::{DevinError, Result};

/// APFEL_BASE: Backend URL (default localhost:11435)
/// For Ollama: APFEL_BASE=http://localhost:11434
fn base_url() -> String {
    std::env::var("APFEL_BASE").unwrap_or_else(|_| "http://localhost:11435".to_string())
}

/// APFEL_MODEL: Model name (default on-device)
/// For Ollama: APFEL_MODEL=qwen2.5-coder:3b
fn model() -> String {
    std::env::var("APFEL_MODEL").unwrap_or_else(|_| "on-device".to_string())
}

const SYSTEM_PROMPT: &str = "\
You are an AI assistant who knows code well. \
You help with code analysis, bug fixes, refactoring, and explanations. \
When suggesting code, always use markdown code blocks.";

pub enum BackendHandle {
    Managed(Child),  // apfel process started directly by devin
    External,        // external server already running
}

impl Drop for BackendHandle {
    fn drop(&mut self) {
        if let BackendHandle::Managed(child) = self {
            let _ = child.kill();
        }
    }
}

/// Use the server if it's already running, otherwise start apfel.
pub fn ensure_server() -> Result<BackendHandle> {
    if server_reachable() {
        return Ok(BackendHandle::External);
    }

    // Start apfel after checking if it's installed
    check_installed()?;

    let child = Command::new("apfel")
        .args([
            "--server",
            "--port", "11435",
            "--context-strategy", "summarize",
            "--context-output-reserve", "600",
            "--system", SYSTEM_PROMPT,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Wait up to 10 seconds for the server to be ready
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(500));
        if server_reachable() {
            return Ok(BackendHandle::Managed(child));
        }
    }

    Err(DevinError::ApfelTimeout)
}

fn server_reachable() -> bool {
    reqwest::blocking::Client::new()
        .get(format!("{}/v1/models", base_url()))
        .timeout(Duration::from_secs(1))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn check_installed() -> Result<()> {
    Command::new("apfel")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| if s.success() { Ok(()) } else { Err(DevinError::ApfelNotFound) })
        .unwrap_or(Err(DevinError::ApfelNotFound))
}

pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new() -> Self {
        Self { http: reqwest::blocking::Client::new() }
    }

    /// Send message list and return response text
    pub fn complete(&self, messages: &[Message]) -> Result<String> {
        let msgs: Vec<Value> = messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();

        let body = json!({
            "model": model(),
            "messages": msgs,
            "stream": false
        });

        let res: Value = self
            .http
            .post(format!("{}/v1/chat/completions", base_url()))
            .json(&body)
            .send()?
            .json()?;

        let text = extract_content(&res).unwrap_or_default();
        if text.is_empty() {
            // Debug: Print raw server response
            eprintln!("[debug] Empty response. Server response: {res}");
        }
        Ok(text)
    }
}

#[derive(Clone)]
pub struct Message {
    pub role: &'static str,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user", content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant", content: content.into() }
    }
}

pub fn extract_content(res: &Value) -> Option<String> {
    res["choices"]
        .as_array()?
        .first()?["message"]["content"]
        .as_str()
        .map(String::from)
}

/// Read a list of file paths and combine them into a context string
pub fn build_file_context(files: &[String]) -> String {
    files
        .iter()
        .filter_map(|path| {
            let content = std::fs::read_to_string(path).ok()?;
            Some(format!("```// {path}\n{content}\n```"))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_content() {
        let res = json!({
            "choices": [{"message": {"content": "Hello"}}]
        });
        assert_eq!(extract_content(&res).unwrap(), "Hello");
    }

    #[test]
    fn test_extract_content_empty() {
        let res = json!({"choices": []});
        assert!(extract_content(&res).is_none());
    }

    #[test]
    fn test_build_file_context_missing_file() {
        let ctx = build_file_context(&["nonexistent.rs".to_string()]);
        assert!(ctx.is_empty());
    }
}
