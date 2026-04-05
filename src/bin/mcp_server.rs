use std::collections::HashMap;
use std::process::Command;

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    schemars,
};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

// ── manifest ──────────────────────────────────────────────────────────────────

/// file_path → list of anchordb chunk IDs
type Manifest = HashMap<String, Vec<u64>>;

fn load_manifest(path: &str) -> Result<Manifest> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

// ── scoring ───────────────────────────────────────────────────────────────────

/// Count how many whitespace-separated tokens from `query` appear (case-insensitive) in `text`.
fn score_text(query: &str, text: &str) -> usize {
    let text_lower = text.to_lowercase();
    query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .filter(|t| text_lower.contains(&t.to_lowercase()))
        .count()
}

/// Return chunk IDs ordered by how well their file path matches the query.
/// Stops adding IDs once the estimated token count exceeds `token_budget`.
fn select_chunk_ids(query: &str, manifest: &Manifest, token_budget: usize) -> Vec<u64> {
    let mut scored: Vec<(&String, &Vec<u64>, usize)> = manifest
        .iter()
        .map(|(file, ids)| (file, ids, score_text(query, file)))
        .collect();
    scored.sort_by(|a, b| b.2.cmp(&a.2));

    let mut ids = Vec::new();
    let mut tokens_used = 0usize;
    'outer: for (_, chunk_ids, _) in scored {
        for id in chunk_ids {
            if tokens_used + 64 > token_budget {
                break 'outer;
            }
            ids.push(*id);
            tokens_used += 64;
        }
    }
    ids
}

// ── anchordb integration ──────────────────────────────────────────────────────

fn load_chunk(db_path: &str, id: u64) -> Option<String> {
    let output = Command::new("anchordb-cli")
        .args([db_path, "load", &id.to_string()])
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

// ── MCP server ────────────────────────────────────────────────────────────────

const TOKEN_BUDGET: usize = 2500;

#[derive(Clone)]
struct ContextServer {
    manifest_path: String,
    db_path: String,
    tool_router: ToolRouter<ContextServer>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetRelevantCodeParams {
    /// Natural-language query describing what code to find.
    query: String,
    /// Optional file path hint to restrict search to a specific file.
    #[serde(default)]
    file_hint: String,
}

#[tool_router]
impl ContextServer {
    fn new(manifest_path: String, db_path: String) -> Self {
        Self {
            manifest_path,
            db_path,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Returns source code chunks relevant to the query. Use file_hint to restrict results to a specific file.")]
    async fn get_relevant_code(
        &self,
        Parameters(params): Parameters<GetRelevantCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        let manifest = load_manifest(&self.manifest_path).map_err(|e| {
            McpError::internal_error(
                "manifest_load_failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

        let effective_manifest: Manifest = if params.file_hint.is_empty() {
            manifest
        } else {
            manifest
                .into_iter()
                .filter(|(file, _)| file.contains(&params.file_hint))
                .collect()
        };

        let ids = select_chunk_ids(&params.query, &effective_manifest, TOKEN_BUDGET);

        if ids.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No relevant code chunks found.",
            )]));
        }

        let mut parts = Vec::new();
        let mut char_budget = TOKEN_BUDGET * 4;

        for id in ids {
            if char_budget == 0 {
                break;
            }
            if let Some(text) = load_chunk(&self.db_path, id) {
                let trimmed = if text.len() > char_budget {
                    text[..char_budget].to_string()
                } else {
                    text.clone()
                };
                char_budget = char_budget.saturating_sub(trimmed.len());
                parts.push(trimmed);
            }
        }

        Ok(CallToolResult::success(vec![Content::text(parts.join("\n\n"))]))
    }
}

#[tool_handler]
impl ServerHandler for ContextServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder().enable_tools().build(),
        )
        .with_server_info(Implementation::new("devin-mcp", env!("CARGO_PKG_VERSION")))
    }
}

// ── entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let manifest_path = std::env::var("DEVIN_MANIFEST_PATH")
        .unwrap_or_else(|_| format!("{}/.local/share/devin/manifest.json",
            std::env::var("HOME").unwrap_or_default()));
    let db_path = std::env::var("DEVIN_DB_PATH")
        .unwrap_or_else(|_| format!("{}/.local/share/devin/chunks.db",
            std::env::var("HOME").unwrap_or_default()));

    let service = ContextServer::new(manifest_path, db_path)
        .serve(rmcp::transport::stdio())
        .await?;

    service.waiting().await?;
    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_counts_matching_tokens() {
        assert_eq!(score_text("foo bar", "foo baz qux"), 1);
        assert_eq!(score_text("foo bar", "foo bar baz"), 2);
        assert_eq!(score_text("foo bar", "nothing here"), 0);
    }

    #[test]
    fn score_is_case_insensitive() {
        assert_eq!(score_text("Foo BAR", "foo bar baz"), 2);
    }

    #[test]
    fn empty_query_scores_zero() {
        assert_eq!(score_text("", "foo bar baz"), 0);
    }

    #[test]
    fn select_returns_higher_scored_files_first() {
        let mut manifest = Manifest::new();
        manifest.insert("src/parser.rs".into(), vec![1, 2]);
        manifest.insert("src/main.rs".into(), vec![3]);

        // "parser" matches "src/parser.rs" but not "src/main.rs"
        let ids = select_chunk_ids("parser", &manifest, 10000);
        assert_eq!(ids[0], 1);
        assert_eq!(ids[1], 2);
        assert_eq!(ids[2], 3);
    }

    #[test]
    fn select_respects_token_budget() {
        let mut manifest = Manifest::new();
        manifest.insert("src/a.rs".into(), vec![1, 2, 3, 4, 5]);

        // budget of 100 tokens / 64 per chunk → only 1 chunk fits
        let ids = select_chunk_ids("anything", &manifest, 100);
        assert_eq!(ids.len(), 1);
    }
}
