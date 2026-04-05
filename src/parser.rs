use regex::Regex;

#[derive(Debug, Clone)]
pub struct CodeChunk {
    pub file: String,
    pub name: String,
    pub kind: String,
    pub body: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub fn parse_file(path: &str, source: &str) -> Vec<CodeChunk> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "rs" => parse_rust(path, source),
        "py" => parse_python(path, source),
        _ => vec![whole_file_chunk(path, source)],
    }
}

fn parse_rust(path: &str, source: &str) -> Vec<CodeChunk> {
    let re = Regex::new(
        r"(?m)^(?:pub\s+)?(?:async\s+)?(fn|struct|enum|impl|trait)\s+(\w+)"
    ).unwrap();
    extract_chunks(path, source, &re, 1, 2)
}

fn parse_python(path: &str, source: &str) -> Vec<CodeChunk> {
    let re = Regex::new(r"(?m)^(?:async\s+)?(def|class)\s+(\w+)").unwrap();
    extract_chunks(path, source, &re, 1, 2)
}

fn extract_chunks(
    path: &str,
    source: &str,
    re: &Regex,
    kind_group: usize,
    name_group: usize,
) -> Vec<CodeChunk> {
    let matches: Vec<_> = re.find_iter(source).collect();
    if matches.is_empty() {
        return vec![whole_file_chunk(path, source)];
    }

    let captures: Vec<_> = re.captures_iter(source).collect();
    let mut chunks = Vec::new();

    for (i, mat) in matches.iter().enumerate() {
        let start_byte = mat.start();
        let end_byte = if i + 1 < matches.len() {
            matches[i + 1].start()
        } else {
            source.len()
        };

        let body = source[start_byte..end_byte].to_string();
        let start_line = source[..start_byte].lines().count() + 1;
        let end_line = start_line + body.lines().count().saturating_sub(1);

        let cap = &captures[i];
        let kind = cap.get(kind_group).map_or("", |m| m.as_str()).to_string();
        let name = cap.get(name_group).map_or("", |m| m.as_str()).to_string();

        chunks.push(CodeChunk {
            file: path.to_string(),
            name,
            kind,
            body,
            start_line,
            end_line,
        });
    }

    chunks
}

fn whole_file_chunk(path: &str, source: &str) -> CodeChunk {
    let line_count = source.lines().count();
    CodeChunk {
        file: path.to_string(),
        name: path.to_string(),
        kind: "file".to_string(),
        body: source.to_string(),
        start_line: 1,
        end_line: line_count.max(1),
    }
}
