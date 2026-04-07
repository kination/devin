use std::path::Path;

use walkdir::WalkDir;

use anchordb::AnchorDB;

use crate::error::Result;
use crate::manifest::Manifest;
use crate::parser::parse_file;

pub struct IndexStats {
    pub files_indexed: usize,
    pub chunks_saved: usize,
    pub files_skipped: usize,
}

const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules", "__pycache__"];
const MAX_FILE_BYTES: u64 = 512 * 1024;

pub fn index_project(root: &Path, db: &AnchorDB, manifest_path: &Path) -> Result<IndexStats> {
    let mut manifest = Manifest::new();
    let mut stats = IndexStats { files_indexed: 0, chunks_saved: 0, files_skipped: 0 };

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_dir() {
            continue;
        }

        let path = entry.path();

        if path_contains_skip_dir(path, root) {
            stats.files_skipped += 1;
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) == Some("lock") {
            stats.files_skipped += 1;
            continue;
        }

        if entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
            stats.files_skipped += 1;
            continue;
        }

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                stats.files_skipped += 1;
                continue;
            }
        };

        let rel_path = path.strip_prefix(root).unwrap_or(path);
        let rel_str = rel_path.to_string_lossy().into_owned();

        let chunks = parse_file(&rel_str, &source);
        let mut ids = Vec::with_capacity(chunks.len());

        for chunk in &chunks {
            let id = db.save(&chunk.body)?;
            ids.push(id);
            stats.chunks_saved += 1;
        }

        manifest.set_file(&rel_str, ids);
        stats.files_indexed += 1;
    }

    manifest.save(manifest_path)?;
    Ok(stats)
}

fn path_contains_skip_dir(path: &Path, root: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| SKIP_DIRS.contains(&s))
            .unwrap_or(false)
    })
}
