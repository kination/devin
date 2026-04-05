use regex::Regex;

pub struct CodeBlock {
    pub content: String,
    pub filename: Option<String>,
}

/// Extract code blocks from a markdown response.
pub fn parse_blocks(text: &str) -> Vec<CodeBlock> {
    let with_name = Regex::new(r"```(?:\w+)?\s*(?://|#)\s*([^\n]+)\n([\s\S]*?)```").unwrap();
    let plain     = Regex::new(r"```(?:\w+)?\n([\s\S]*?)```").unwrap();

    let mut blocks = Vec::new();
    for cap in with_name.captures_iter(text) {
        blocks.push(CodeBlock {
            filename: Some(cap[1].trim().to_string()),
            content:  cap[2].trim_end_matches('\n').to_string(),
        });
    }
    if blocks.is_empty() {
        for cap in plain.captures_iter(text) {
            blocks.push(CodeBlock {
                filename: None,
                content:  cap[1].trim_end_matches('\n').to_string(),
            });
        }
    }
    blocks
}

// ── diff ──────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum DiffKind { Add, Remove, Context }

pub struct DiffLine {
    pub kind:    DiffKind,
    pub content: String,
}

const CONTEXT: usize = 3;

/// Compute a unified diff between `old` and `new` text.
/// Returns only the changed hunks with CONTEXT lines of surrounding context.
pub fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let edits = lcs_diff(&old_lines, &new_lines);
    with_context(edits, CONTEXT)
}

// ── LCS-based diff ────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Edit {
    Keep(String),
    Insert(String),
    Delete(String),
}

fn lcs_diff(old: &[&str], new: &[&str]) -> Vec<Edit> {
    let m = old.len();
    let n = new.len();

    // dp[i][j] = LCS length for old[..i], new[..j]
    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if old[i - 1] == new[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    // Backtrack
    let mut edits = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            edits.push(Edit::Keep(old[i - 1].to_string()));
            i -= 1; j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            edits.push(Edit::Insert(new[j - 1].to_string()));
            j -= 1;
        } else {
            edits.push(Edit::Delete(old[i - 1].to_string()));
            i -= 1;
        }
    }
    edits.reverse();
    edits
}

/// Keep only hunks within CONTEXT lines of a change, with line numbers.
fn with_context(edits: Vec<Edit>, ctx: usize) -> Vec<DiffLine> {
    // First pass: assign new-file line numbers and mark changed positions
    let annotated: Vec<Edit> = edits;

    // Mark indices that are changed
    let changed: Vec<bool> = annotated.iter()
        .map(|e| !matches!(e, Edit::Keep(_)))
        .collect();

    // Build context mask
    let len = annotated.len();
    let mut include = vec![false; len];
    for i in 0..len {
        if changed[i] {
            let lo = i.saturating_sub(ctx);
            let hi = (i + ctx + 1).min(len);
            for k in lo..hi { include[k] = true; }
        }
    }

    let mut result = Vec::new();
    let mut prev_included = false;
    for (idx, edit) in annotated.into_iter().enumerate() {
        if !include[idx] {
            if prev_included {
                result.push(DiffLine { kind: DiffKind::Context, content: "⋯".to_string() });
            }
            prev_included = false;
            continue;
        }
        prev_included = true;
        match edit {
            Edit::Keep(c)   => result.push(DiffLine { kind: DiffKind::Context, content: c }),
            Edit::Insert(c) => result.push(DiffLine { kind: DiffKind::Add,     content: c }),
            Edit::Delete(c) => result.push(DiffLine { kind: DiffKind::Remove,  content: c }),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_blocks_with_filename() {
        let text = "```rust\n// src/main.rs\nfn main() {}\n```";
        let blocks = parse_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].filename.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn test_parse_blocks_no_filename() {
        let text = "```rust\nfn main() {}\n```";
        let blocks = parse_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].filename.is_none());
    }

    #[test]
    fn test_compute_diff_addition() {
        let old = "a\nb\nc";
        let new = "a\nb\nx\nc";
        let diff = compute_diff(old, new);
        let adds: Vec<_> = diff.iter().filter(|l| l.kind == DiffKind::Add).collect();
        assert_eq!(adds.len(), 1);
        assert_eq!(adds[0].content, "x");
    }

    #[test]
    fn test_compute_diff_no_change() {
        let s = "a\nb\nc";
        let diff = compute_diff(s, s);
        assert!(diff.iter().all(|l| l.kind == DiffKind::Context || l.content == "⋯"));
    }
}
