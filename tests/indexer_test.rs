use std::fs;
use entic::indexer::{index_project, IndexStats};
use entic::manifest::Manifest;
use anchordb::AnchorDB;
use tempfile::tempdir;

fn write_fixture(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/lib.rs"), "pub fn foo() {}\npub fn bar() {}\n").unwrap();
    fs::write(dir.join("src/main.py"), "def run():\n    pass\n").unwrap();
    // These should be skipped
    fs::create_dir_all(dir.join("target")).unwrap();
    fs::write(dir.join("target/debug.bin"), "binary").unwrap();
    fs::write(dir.join("Cargo.lock"), "lock file").unwrap();
}

#[test]
fn test_index_project_stats() {
    let dir = tempdir().unwrap();
    write_fixture(dir.path());

    let db_path = dir.path().join("chunks.db");
    let manifest_path = dir.path().join("manifest.json");
    let db = AnchorDB::open(&db_path).unwrap();

    let stats = index_project(dir.path(), &db, &manifest_path).unwrap();

    assert_eq!(stats.files_indexed, 2, "should index lib.rs and main.py");
    assert!(stats.chunks_saved >= 2, "at least 2 chunks (foo, bar)");
    assert!(stats.files_skipped >= 2, "target/ and Cargo.lock skipped");
}

#[test]
fn test_manifest_has_correct_entries() {
    let dir = tempdir().unwrap();
    write_fixture(dir.path());

    let db_path = dir.path().join("chunks.db");
    let manifest_path = dir.path().join("manifest.json");
    let db = AnchorDB::open(&db_path).unwrap();

    index_project(dir.path(), &db, &manifest_path).unwrap();

    let manifest = Manifest::load(&manifest_path).unwrap();
    let keys: Vec<_> = manifest.entries.keys().collect();

    assert!(
        keys.iter().any(|k| k.ends_with("lib.rs")),
        "manifest should contain lib.rs, got: {keys:?}"
    );
    assert!(
        keys.iter().any(|k| k.ends_with("main.py")),
        "manifest should contain main.py, got: {keys:?}"
    );
}

#[test]
fn test_chunks_stored_in_db() {
    let dir = tempdir().unwrap();
    write_fixture(dir.path());

    let db_path = dir.path().join("chunks.db");
    let manifest_path = dir.path().join("manifest.json");
    let db = AnchorDB::open(&db_path).unwrap();

    index_project(dir.path(), &db, &manifest_path).unwrap();

    let manifest = Manifest::load(&manifest_path).unwrap();
    let all_ids: Vec<u64> = manifest.entries.values().flatten().copied().collect();
    assert!(!all_ids.is_empty());

    for id in &all_ids {
        let chunk = db.load(*id).unwrap();
        assert!(chunk.is_some(), "chunk {id} should exist in db");
    }
}
