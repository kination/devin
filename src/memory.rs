use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MemoryStore {
    #[serde(default)]
    pub conventions: Vec<String>,
    #[serde(default)]
    pub gotchas: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(skip)]
    path: PathBuf,
}

impl MemoryStore {
    /// Load from `.devin-memory.json` in the current directory.
    pub fn load() -> Self {
        Self::load_from(PathBuf::from(".devin-memory.json"))
    }

    pub fn load_from(path: PathBuf) -> Self {
        let mut store: MemoryStore = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        store.path = path;
        store
    }

    pub fn save(&self) {
        self.save_to(&self.path);
    }

    pub fn save_to(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Insert a memory entry. Returns false if duplicate or unknown category.
    /// Duplicate: new content is a substring of an existing entry, or vice versa.
    pub fn insert(&mut self, category: &str, content: String) -> bool {
        if content.trim().is_empty() {
            return false;
        }
        let bucket = match category {
            "conventions" => &mut self.conventions,
            "gotchas"     => &mut self.gotchas,
            "decisions"   => &mut self.decisions,
            "patterns"    => &mut self.patterns,
            _             => return false,
        };
        let c = content.trim();
        for existing in bucket.iter() {
            if existing.contains(c) || c.contains(existing.as_str()) {
                return false;
            }
        }
        bucket.push(content.trim().to_string());
        true
    }

    /// Format all non-empty categories for prompt injection.
    /// Returns empty string if all categories are empty.
    pub fn build_context(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        for (label, bucket) in [
            ("conventions", &self.conventions),
            ("gotchas",     &self.gotchas),
            ("decisions",   &self.decisions),
            ("patterns",    &self.patterns),
        ] {
            if bucket.is_empty() { continue; }
            let entries = bucket.iter()
                .map(|e| format!("- {e}"))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("{label}:\n{entries}"));
        }

        if parts.is_empty() {
            return String::new();
        }

        format!("[PROJECT MEMORY]\n{}", parts.join("\n\n"))
    }

    /// Extract and save a <MEMORY> block from the LLM response.
    /// Returns the response with the block removed.
    pub fn parse_memory_block(&mut self, response: &str) -> String {
        use regex::Regex;
        let re = Regex::new(r"(?s)<MEMORY>\s*(.*?)\s*</MEMORY>").unwrap();

        let Some(cap) = re.captures(response) else {
            return response.to_string();
        };

        let json_str = &cap[1];
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
            let category = v["category"].as_str().unwrap_or("");
            let content  = v["content"].as_str().unwrap_or("");
            if self.insert(category, content.to_string()) {
                self.save();
            }
        }

        let cleaned = re.replace(response, "");
        cleaned.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store(dir: &TempDir) -> MemoryStore {
        let path = dir.path().join(".devin-memory.json");
        MemoryStore::load_from(path)
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let s = temp_store(&dir);
        assert!(s.conventions.is_empty());
        assert!(s.gotchas.is_empty());
        assert!(s.decisions.is_empty());
        assert!(s.patterns.is_empty());
    }

    #[test]
    fn build_context_empty_returns_empty_string() {
        let dir = TempDir::new().unwrap();
        let s = temp_store(&dir);
        assert_eq!(s.build_context(), "");
    }

    #[test]
    fn build_context_includes_nonempty_categories() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        s.conventions.push("use Result everywhere".to_string());
        s.gotchas.push("apfel needs polling".to_string());
        let ctx = s.build_context();
        assert!(ctx.contains("[PROJECT MEMORY]"));
        assert!(ctx.contains("conventions:"));
        assert!(ctx.contains("use Result everywhere"));
        assert!(ctx.contains("gotchas:"));
        assert!(ctx.contains("apfel needs polling"));
        assert!(!ctx.contains("decisions:"));
        assert!(!ctx.contains("patterns:"));
    }

    #[test]
    fn parse_memory_block_extracts_and_removes() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        let raw = "Here is my answer.\n\n<MEMORY>\n{\"category\": \"gotchas\", \"content\": \"foo bar\"}\n</MEMORY>";
        let display = s.parse_memory_block(raw);
        assert!(!display.contains("<MEMORY>"));
        assert!(!display.contains("foo bar"));
        assert!(display.contains("Here is my answer."));
        assert_eq!(s.gotchas, vec!["foo bar"]);
    }

    #[test]
    fn parse_memory_block_no_block_returns_unchanged() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        let raw = "Just a normal response.";
        let display = s.parse_memory_block(raw);
        assert_eq!(display, raw);
        assert!(s.gotchas.is_empty());
    }

    #[test]
    fn parse_memory_block_malformed_json_ignored() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        let raw = "Answer.\n\n<MEMORY>\nnot json\n</MEMORY>";
        let display = s.parse_memory_block(raw);
        assert!(!display.contains("<MEMORY>"));
        assert!(s.conventions.is_empty());
    }

    #[test]
    fn insert_adds_new_entry() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        let added = s.insert("gotchas", "watch out for X".to_string());
        assert!(added);
        assert_eq!(s.gotchas, vec!["watch out for X"]);
    }

    #[test]
    fn insert_rejects_exact_duplicate() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        s.insert("gotchas", "watch out for X".to_string());
        let added = s.insert("gotchas", "watch out for X".to_string());
        assert!(!added);
        assert_eq!(s.gotchas.len(), 1);
    }

    #[test]
    fn insert_rejects_substring() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        s.insert("conventions", "use Result everywhere".to_string());
        let added = s.insert("conventions", "use Result".to_string());
        assert!(!added);
    }

    #[test]
    fn insert_rejects_superset() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        s.insert("conventions", "use Result".to_string());
        let added = s.insert("conventions", "always use Result everywhere".to_string());
        assert!(!added);
    }

    #[test]
    fn insert_unknown_category_returns_false() {
        let dir = TempDir::new().unwrap();
        let mut s = temp_store(&dir);
        let added = s.insert("unknown", "something".to_string());
        assert!(!added);
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".devin-memory.json");
        let mut s = MemoryStore::load_from(path.clone());
        s.conventions.push("use Result everywhere".to_string());
        s.save_to(&path);

        let s2 = MemoryStore::load_from(path);
        assert_eq!(s2.conventions, vec!["use Result everywhere"]);
    }
}
