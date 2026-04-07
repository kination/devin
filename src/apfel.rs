use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

use crate::error::{DevinError, Result};

/// APFEL_BASE: backend URL.
/// Default: localhost:11435 (devin-managed apfel, avoids Ollama on 11434).
/// Override: APFEL_BASE=http://localhost:11434 to use Ollama or another server.
fn base_url() -> String {
    std::env::var("APFEL_BASE").unwrap_or_else(|_| "http://localhost:11435".to_string())
}

/// APFEL_MODEL: model name to send in API requests.
/// When unset, auto-detected from the running server's /v1/models list.
/// apfel returns its built-in model; Ollama returns whatever is loaded.
pub fn model() -> String {
    if let Ok(m) = std::env::var("APFEL_MODEL") {
        return m;
    }
    detect_model_from_server().unwrap_or_else(|| "on-device".to_string())
}

fn detect_model_from_server() -> Option<String> {
    let res = reqwest::blocking::Client::new()
        .get(format!("{}/v1/models", base_url()))
        .timeout(Duration::from_secs(2))
        .send()
        .ok()?;
    let v: serde_json::Value = res.json().ok()?;
    v["data"].as_array()?.first()?["id"].as_str().map(String::from)
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

/// Returns the devin-mcp binary path if it exists alongside the running devin binary.
fn mcp_binary_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let mcp = dir.join("devin-mcp");
    if mcp.is_file() { Some(mcp) } else { None }
}

/// Returns true if manifest.json exists at the default location (index has been built).
fn index_exists() -> bool {
    crate::paths::default_manifest_path().exists()
}

/// Use the server if it's already running, otherwise start apfel.
pub fn ensure_server() -> Result<BackendHandle> {
    if server_reachable() {
        return Ok(BackendHandle::External);
    }

    check_installed()?;

    let db_path = crate::paths::default_db_path();
    let manifest_path = crate::paths::default_manifest_path();

    let mut cmd = Command::new("apfel");
    cmd.args([
        "--serve",
        "--port", "11435",
        "--context-strategy", "summarize",
        "--context-output-reserve", "600",
        "--system", SYSTEM_PROMPT,
    ]);

    // Wire devin-mcp via apfel's --mcp <path> flag.
    // apfel --help output: --mcp <path>   Attach MCP tool server (repeatable)
    // apfel spawns the binary directly; env vars are passed via the apfel process environment.
    if let Some(mcp_bin) = mcp_binary_path() {
        if index_exists() {
            cmd.env("DEVIN_DB_PATH", &db_path);
            cmd.env("DEVIN_MANIFEST_PATH", &manifest_path);
            cmd.args(["--mcp", mcp_bin.to_str().unwrap_or("devin-mcp")]);
        } else {
            eprintln!("  devin: index not found — run `devin index <path>` for code context");
        }
    } else {
        eprintln!("  devin: devin-mcp binary not found — MCP context disabled");
    }

    let child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

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

    /// Send messages and return full response text (non-streaming)
    pub fn complete(&self, messages: &[Message]) -> Result<String> {
        let msgs = self.build_msgs(messages);
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

        Ok(extract_content(&res).unwrap_or_default())
    }

    /// Stream response tokens, calling `on_token` for each chunk.
    /// Returns the full response string.
    pub fn stream(&self, messages: &[Message], mut on_token: impl FnMut(&str)) -> Result<String> {
        use std::io::BufRead;

        let msgs = self.build_msgs(messages);
        let body = json!({
            "model": model(),
            "messages": msgs,
            "stream": true
        });

        let res = self
            .http
            .post(format!("{}/v1/chat/completions", base_url()))
            .json(&body)
            .send()?;

        let mut full = String::new();

        for line in std::io::BufReader::new(res).lines() {
            let line = line.map_err(std::io::Error::from)?;
            let data = match line.strip_prefix("data: ") {
                Some(d) => d,
                None => continue,
            };
            if data == "[DONE]" { break; }

            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(token) = v["choices"][0]["delta"]["content"].as_str() {
                    on_token(token);
                    full.push_str(token);
                }
            }
        }

        Ok(full)
    }

    fn build_msgs(&self, messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect()
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

/// Scan text for absolute file paths that exist on disk.
/// Used to auto-attach files the user mentions in their prompt.
pub fn extract_mentioned_files(text: &str) -> Vec<String> {
    use regex::Regex;
    // Match absolute paths: /some/path/file.ext
    // Stops at whitespace, quotes, or common punctuation that wouldn't be part of a path.
    let re = Regex::new(r#"/[^\s'"<>(),;]+\.[a-zA-Z0-9]+"#).unwrap();
    re.find_iter(text)
        .map(|m| {
            m.as_str()
                .trim_end_matches(|c: char| matches!(c, '.' | ',' | ')' | ']'))
                .to_string()
        })
        .filter(|p| std::path::Path::new(p).is_file())
        .collect()
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
