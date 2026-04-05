use regex::Regex;

/// Extracts markdown code blocks from the assistant's response.
pub fn extract_code_blocks(text: &str) -> Vec<String> {
    let re = Regex::new(r"```(?:\w+)?\n([\s\S]*?)```").unwrap();
    re.captures_iter(text)
        .map(|cap| cap[1].trim_end_matches('\n').to_string())
        .collect()
}

/// A simple struct to represent an applyable change.
pub struct CodeBlock {
    pub content: String,
    pub filename: Option<String>,
}

pub fn parse_blocks(text: &str) -> Vec<CodeBlock> {
    let re = Regex::new(r"```(?:\w+)?\s*(?://|#)\s*([^\n]+)\n([\s\S]*?)```").unwrap();
    let mut blocks = Vec::new();

    for cap in re.captures_iter(text) {
        blocks.push(CodeBlock {
            filename: Some(cap[1].trim().to_string()),
            content: cap[2].trim_end_matches('\n').to_string(),
        });
    }

    // Fallback if no filename comment:
    if blocks.is_empty() {
        for b in extract_code_blocks(text) {
            blocks.push(CodeBlock { content: b, filename: None });
        }
    }

    blocks
}
