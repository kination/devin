use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::LazyLock;
use regex::Regex;

static PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"@?(/[^\s'"<>(),;]+|\.{1,2}/[^\s'"<>(),;]+|[a-zA-Z0-9_][a-zA-Z0-9_\-]*/[^\s'"<>(),;]*|[a-zA-Z0-9_\-]+\.[a-zA-Z0-9]+)"#,
    )
    .expect("invalid regex")
});

#[derive(Debug, Clone, PartialEq)]
pub enum PathKind {
    File,
    Dir,
}

#[derive(Debug, Clone)]
pub struct DetectedPath {
    pub raw: String,
    pub resolved: PathBuf,
    pub kind: PathKind,
}

#[derive(Default)]
pub struct PermissionSet {
    pub approved: HashSet<PathBuf>,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self {
            approved: HashSet::new(),
        }
    }

    pub fn is_approved(&self, p: &std::path::Path) -> bool {
        self.approved.contains(p)
    }

    pub fn approve(&mut self, p: PathBuf) {
        self.approved.insert(p);
    }
}

const MAX_FILE_BYTES: u64 = 512 * 1024;

/// Read content from a DetectedPath.
/// Files: return text content (skip if > 512KB or non-UTF-8).
/// Dirs: list immediate children + read each text file up to cumulative cap.
pub fn read_path(p: &DetectedPath) -> String {
    match p.kind {
        PathKind::File => read_file_content(&p.resolved),
        PathKind::Dir  => read_dir_content(&p.resolved),
    }
}

fn read_file_content(path: &PathBuf) -> String {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return format!("# {}\n[error: {e}]\n\n", path.display()),
    };
    if meta.len() > MAX_FILE_BYTES {
        return format!("# {}\n[skipped: too large ({} bytes)]\n\n", path.display(), meta.len());
    }
    match std::fs::read_to_string(path) {
        Ok(content) => format!("# {}\n{content}\n\n", path.display()),
        Err(_) => format!("# {}\n[skipped: binary file]\n\n", path.display()),
    }
}

fn read_dir_content(path: &PathBuf) -> String {
    let mut out = format!("# {} (directory)\n", path.display());
    let mut cumulative: u64 = 0;

    let mut entries: Vec<_> = match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            out.push_str(&format!("[error: {e}]\n\n"));
            return out;
        }
    };
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let ep = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if ep.is_dir() {
            out.push_str(&format!("  [dir]  {name}\n"));
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push_str(&format!("  [file] {name} ({size} bytes)\n"));
    }
    out.push('\n');

    // Read file contents up to cumulative cap
    for entry in &entries {
        let ep = entry.path();
        if !ep.is_file() { continue; }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if cumulative + size > MAX_FILE_BYTES { continue; }
        if let Ok(text) = std::fs::read_to_string(&ep) {
            out.push_str(&format!("# {}\n{text}\n\n", ep.display()));
            cumulative += size;
        }
    }

    out
}

/// Detect file/dir paths mentioned in `input`.
/// When a bare name doesn't resolve directly, searches the project tree.
pub fn detect_paths(input: &str) -> Vec<DetectedPath> {
    use std::env;

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut results: Vec<DetectedPath> = Vec::new();

    for m in PATH_RE.find_iter(input) {
        let raw = m.as_str().to_string();
        let path_str = raw.strip_prefix('@').unwrap_or(&raw);

        let candidate = if path_str.starts_with('/') {
            PathBuf::from(path_str)
        } else {
            cwd.join(path_str)
        };

        match candidate.canonicalize() {
            Ok(resolved) => {
                if seen.insert(resolved.clone()) {
                    let kind = if resolved.is_dir() { PathKind::Dir } else { PathKind::File };
                    results.push(DetectedPath { raw, resolved, kind });
                }
            }
            Err(_) => {
                // Direct resolution failed. For bare names (no '/' except trailing),
                // fall back to a project-wide search.
                let normalized = path_str.trim_end_matches('/');
                if !normalized.contains('/') {
                    for found in search_project_for_name(normalized) {
                        if seen.insert(found.resolved.clone()) {
                            results.push(found);
                        }
                    }
                }
            }
        }
    }

    results
}

const SEARCH_LIMIT: usize = 5;

static IGNORED_DIRS: &[&str] = &[
    ".git", "target", "node_modules", ".build", "dist", ".next", "__pycache__", ".cache",
];

/// Search the project tree (from cwd) for files or directories whose name exactly
/// matches `name`. Returns up to SEARCH_LIMIT results.
pub fn search_project_for_name(name: &str) -> Vec<DetectedPath> {
    use walkdir::WalkDir;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let name_lower = name.to_lowercase();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut results = Vec::new();

    'walk: for entry in WalkDir::new(&cwd)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_str().unwrap_or("");
            !IGNORED_DIRS.contains(&n)
        })
        .filter_map(|e| e.ok())
        .skip(1) // skip cwd itself
    {
        let file_name = entry.file_name().to_str().unwrap_or("");
        if file_name.to_lowercase() == name_lower {
            if let Ok(resolved) = entry.path().canonicalize() {
                if seen.insert(resolved.clone()) {
                    let kind = if resolved.is_dir() { PathKind::Dir } else { PathKind::File };
                    results.push(DetectedPath {
                        raw: name.to_string(),
                        resolved,
                        kind,
                    });
                    if results.len() >= SEARCH_LIMIT {
                        break 'walk;
                    }
                }
            }
        }
    }

    results
}

/// Detect paths in `input`, request permission via `confirm_fn` for new ones,
/// read approved paths, and return the injected context string.
/// `confirm_fn` receives a slice of paths needing approval; returns those approved.
pub fn enrich<F>(input: &str, permissions: &mut PermissionSet, confirm_fn: F) -> String
where
    F: FnOnce(&[DetectedPath]) -> Vec<DetectedPath>,
{
    let all_paths = detect_paths(input);
    if all_paths.is_empty() {
        return String::new();
    }

    let (already_approved, needs_prompt): (Vec<_>, Vec<_>) = all_paths
        .into_iter()
        .partition(|p| permissions.is_approved(&p.resolved));

    let newly_approved = if needs_prompt.is_empty() {
        vec![]
    } else {
        let approved = confirm_fn(&needs_prompt);
        for p in &approved {
            permissions.approve(p.resolved.clone());
        }
        approved
    };

    let to_read: Vec<_> = already_approved.into_iter().chain(newly_approved).collect();

    to_read.iter().map(|p| read_path(p)).collect::<String>()
}

pub const REINJECT_LIMIT: usize = 3;

pub struct SessionContext {
    pub files: Vec<DetectedPath>,
    seen: HashSet<PathBuf>,
}

impl SessionContext {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// Per-turn entry point.
    ///
    /// - `detected` non-empty: prepend new unique paths to cache, return `detected`.
    /// - `detected` empty: return up to `REINJECT_LIMIT` most-recent cached paths.
    pub fn resolve(&mut self, detected: Vec<DetectedPath>) -> Vec<DetectedPath> {
        if detected.is_empty() {
            return self.files.iter().take(REINJECT_LIMIT).cloned().collect();
        }
        // Prepend in reverse so the first item in `detected` ends up at index 0.
        for path in detected.iter().rev() {
            if self.seen.insert(path.resolved.clone()) {
                self.files.insert(0, path.clone());
            }
        }
        detected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detects_absolute_file() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("main.rs");
        fs::write(&f, "fn main() {}").unwrap();
        let input = format!("review {}", f.display());
        let paths = detect_paths(&input);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].kind, PathKind::File);
    }

    #[test]
    fn detects_absolute_dir() {
        let dir = TempDir::new().unwrap();
        let input = format!("review {}", dir.path().display());
        let paths = detect_paths(&input);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].kind, PathKind::Dir);
    }

    #[test]
    fn detects_at_mention_file() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("lib.rs");
        fs::write(&f, "").unwrap();
        let input = format!("check @{}", f.display());
        let paths = detect_paths(&input);
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn ignores_nonexistent_paths() {
        let paths = detect_paths("look at /nonexistent/path/foo.rs");
        assert!(paths.is_empty());
    }

    #[test]
    fn reads_file_content() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("foo.rs");
        fs::write(&f, "fn foo() {}").unwrap();
        let dp = DetectedPath {
            raw: f.to_string_lossy().into(),
            resolved: f.canonicalize().unwrap(),
            kind: PathKind::File,
        };
        let out = read_path(&dp);
        assert!(out.contains("fn foo()"));
        assert!(out.contains("foo.rs"));
    }

    #[test]
    fn skips_oversized_file() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("big.rs");
        fs::write(&f, "x".repeat(600 * 1024)).unwrap();
        let dp = DetectedPath {
            raw: f.to_string_lossy().into(),
            resolved: f.canonicalize().unwrap(),
            kind: PathKind::File,
        };
        let out = read_path(&dp);
        assert!(out.contains("skipped") || out.contains("too large"));
    }

    #[test]
    fn lists_dir_children() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        fs::write(dir.path().join("b.rs"), "fn b() {}").unwrap();
        let dp = DetectedPath {
            raw: dir.path().to_string_lossy().into(),
            resolved: dir.path().canonicalize().unwrap(),
            kind: PathKind::Dir,
        };
        let out = read_path(&dp);
        assert!(out.contains("a.rs"));
        assert!(out.contains("b.rs"));
    }

    #[test]
    fn deduplicates_same_path() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("a.rs");
        fs::write(&f, "").unwrap();
        let p = f.display();
        let input = format!("{p} and again {p}");
        let paths = detect_paths(&input);
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn enrich_injects_approved_file() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("hello.rs");
        fs::write(&f, "fn hello() {}").unwrap();
        let mut perms = PermissionSet::new();
        let input = format!("review {}", f.display());
        let ctx = enrich(&input, &mut perms, |paths| paths.to_vec());
        assert!(ctx.contains("fn hello()"));
    }

    #[test]
    fn enrich_skips_denied_path() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("secret.rs");
        fs::write(&f, "secret").unwrap();
        let mut perms = PermissionSet::new();
        let input = format!("review {}", f.display());
        let ctx = enrich(&input, &mut perms, |_| vec![]);
        assert!(!ctx.contains("secret"));
    }

    #[test]
    fn enrich_skips_prompt_for_already_approved() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("cached.rs");
        fs::write(&f, "fn cached() {}").unwrap();
        let resolved = f.canonicalize().unwrap();
        let mut perms = PermissionSet::new();
        perms.approve(resolved);
        let input = format!("review {}", f.display());
        let mut prompt_called = false;
        let ctx = enrich(&input, &mut perms, |paths| {
            prompt_called = true;
            paths.to_vec()
        });
        assert!(!prompt_called, "should not prompt for already-approved path");
        assert!(ctx.contains("fn cached()"));
    }

    // ── SessionContext tests ──────────────────────────────────────────────────

    fn make_detected(dir: &TempDir, name: &str) -> DetectedPath {
        let path = dir.path().join(name);
        fs::write(&path, "content").unwrap();
        DetectedPath {
            raw: name.to_string(),
            resolved: path.canonicalize().unwrap(),
            kind: PathKind::File,
        }
    }

    #[test]
    fn session_resolve_new_paths_returns_them_and_updates_cache() {
        let dir = TempDir::new().unwrap();
        let mut ctx = SessionContext::new();
        let a = make_detected(&dir, "a.rs");
        let result = ctx.resolve(vec![a.clone()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].resolved, a.resolved);
        assert_eq!(ctx.files.len(), 1);
    }

    #[test]
    fn session_resolve_empty_returns_cached_files() {
        let dir = TempDir::new().unwrap();
        let mut ctx = SessionContext::new();
        let a = make_detected(&dir, "a.rs");
        ctx.resolve(vec![a.clone()]);
        let result = ctx.resolve(vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].resolved, a.resolved);
    }

    #[test]
    fn session_resolve_deduplicates_same_path() {
        let dir = TempDir::new().unwrap();
        let mut ctx = SessionContext::new();
        let a = make_detected(&dir, "a.rs");
        ctx.resolve(vec![a.clone()]);
        ctx.resolve(vec![a.clone()]);
        assert_eq!(ctx.files.len(), 1);
    }

    #[test]
    fn session_resolve_reinject_capped_at_limit() {
        let dir = TempDir::new().unwrap();
        let mut ctx = SessionContext::new();
        for i in 0..5usize {
            let p = make_detected(&dir, &format!("{i}.rs"));
            ctx.resolve(vec![p]);
        }
        let result = ctx.resolve(vec![]);
        assert_eq!(result.len(), REINJECT_LIMIT);
    }

    #[test]
    fn session_resolve_new_detection_overrides_reinject() {
        let dir = TempDir::new().unwrap();
        let mut ctx = SessionContext::new();
        let a = make_detected(&dir, "a.rs");
        let b = make_detected(&dir, "b.rs");
        ctx.resolve(vec![a]);
        let result = ctx.resolve(vec![b.clone()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].resolved, b.resolved);
    }
}
