use std::path::PathBuf;

use anchordb::AnchorDB;
use entic::manifest::Manifest;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use rmcp::schemars;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Helper functions (also tested below)
// ---------------------------------------------------------------------------

/// Split text on non-alphanumeric, non-underscore chars; lowercase each token.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// Count how many tokens from `tokens` appear as substrings of `path`.
pub fn score_file(path: &str, tokens: &[&str]) -> usize {
    tokens.iter().filter(|&&t| path.contains(t)).count()
}

/// Chars / 4, rounded down (min 0).
pub fn estimate_tokens(s: &str) -> usize {
    s.len() / 4
}

/// Given `(header, body)` pairs and a token budget, return the prefix that
/// fits within the budget.
pub fn enforce_budget(chunks: Vec<(String, String)>, budget: usize) -> Vec<(String, String)> {
    let mut used = 0usize;
    let mut result = Vec::new();
    for (h, b) in chunks {
        let cost = estimate_tokens(&b);
        if used + cost > budget {
            break;
        }
        used += cost;
        result.push((h, b));
    }
    result
}

// ---------------------------------------------------------------------------
// Tool parameter type
// ---------------------------------------------------------------------------

/// Parameters for the get_relevant_code tool.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RelevantCodeParams {
    /// Search query to find relevant code.
    pub query: String,
    /// Optional file path hint to restrict results.
    pub file_hint: Option<String>,
}

// ---------------------------------------------------------------------------
// Server struct
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct EnticServer {
    db_path: PathBuf,
    manifest_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for EnticServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnticServer")
            .field("db_path", &self.db_path)
            .field("manifest_path", &self.manifest_path)
            .finish()
    }
}

impl EnticServer {
    pub fn new(db_path: PathBuf, manifest_path: PathBuf) -> Self {
        Self {
            db_path,
            manifest_path,
            tool_router: Self::tool_router(),
        }
    }

    /// Synchronous retrieval logic.
    pub fn retrieve(&self, query: &str, file_hint: Option<&str>) -> String {
        let manifest = match Manifest::load(&self.manifest_path) {
            Ok(m) => m,
            Err(_) => return String::new(),
        };

        let db = match AnchorDB::open(&self.db_path) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let tokens: Vec<String> = tokenize(query);
        let token_refs: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();

        // Score and optionally filter files.
        let mut scored: Vec<(usize, String)> = manifest
            .entries
            .keys()
            .filter_map(|file| {
                if let Some(hint) = file_hint {
                    if !file.contains(hint) {
                        return None;
                    }
                }
                let score = score_file(file, &token_refs);
                if score > 0 { Some((score, file.clone())) } else { None }
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));

        // Gather (header, body) pairs for all matching chunks.
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (_score, file) in &scored {
            if let Some(ids) = manifest.get_ids(file) {
                for &id in ids {
                    if let Ok(Some(body)) = db.load(id) {
                        pairs.push((file.clone(), body));
                    }
                }
            }
        }

        let within_budget = enforce_budget(pairs, 2500);

        let mut out = String::new();
        for (file, body) in within_budget {
            out.push_str(&format!("# {file}\n{body}\n\n"));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Tool routing via rmcp macros
// ---------------------------------------------------------------------------

#[tool_router]
impl EnticServer {
    /// Return relevant code chunks for a query.
    #[tool(description = "Return code chunks relevant to a query, optionally filtered by file path hint.")]
    async fn get_relevant_code(
        &self,
        Parameters(params): Parameters<RelevantCodeParams>,
    ) -> String {
        self.retrieve(&params.query, params.file_hint.as_deref())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EnticServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_server_info(Implementation::new(
            "entic-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn default_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("entic")
        .join("chunks.db")
}

fn default_manifest_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("entic")
        .join("manifest.json")
}

#[tokio::main]
async fn main() {
    let db_path = std::env::var("ENTIC_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_db_path());

    let manifest_path = std::env::var("ENTIC_MANIFEST_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_manifest_path());

    let server = EnticServer::new(db_path, manifest_path);
    let transport = stdio();
    server
        .serve(transport)
        .await
        .expect("MCP server failed")
        .waiting()
        .await
        .expect("MCP server waiting failed");
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("parse_blocks in src/diff.rs");
        assert!(tokens.contains(&"parse_blocks".to_string()));
        assert!(tokens.contains(&"src".to_string()));
        assert!(tokens.contains(&"diff".to_string()));
        assert!(tokens.contains(&"rs".to_string()));
    }

    #[test]
    fn test_score_file_match() {
        assert_eq!(score_file("src/diff.rs", &["diff"]), 1);
    }

    #[test]
    fn test_score_file_no_match() {
        assert_eq!(score_file("src/parser.rs", &["diff"]), 0);
    }

    #[test]
    fn test_score_file_multi() {
        assert_eq!(score_file("src/parse_diff.rs", &["parse", "diff"]), 2);
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_enforce_budget_fits() {
        let body = "a".repeat(100);
        let chunks = vec![
            ("h1".to_string(), body.clone()),
            ("h2".to_string(), body.clone()),
        ];
        // Each body is 100 chars = 25 tokens; 25+25=50 < 100 budget
        let result = enforce_budget(chunks, 100);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_enforce_budget_overflow() {
        let big = "a".repeat(8000);
        let med = "a".repeat(4000);
        let chunks = vec![
            ("h1".to_string(), big),
            ("h2".to_string(), med),
        ];
        // 8000/4=2000 tokens fits in 2500; next 4000/4=1000 would push to 3000 > 2500
        let result = enforce_budget(chunks, 2500);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_enforce_budget_empty() {
        let result = enforce_budget(vec![], 2500);
        assert_eq!(result.len(), 0);
    }
}
