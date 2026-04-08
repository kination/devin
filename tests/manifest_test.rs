use entic::manifest::Manifest;
use tempfile::tempdir;

#[test]
fn test_round_trip_empty() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("manifest.json");

    let m = Manifest::new();
    m.save(&path).unwrap();

    let loaded = Manifest::load(&path).unwrap();
    assert_eq!(loaded.entries.len(), 0);
}

#[test]
fn test_set_and_get_ids() {
    let mut m = Manifest::new();
    m.set_file("src/lib.rs", vec![1, 2, 3]);
    assert_eq!(m.get_ids("src/lib.rs"), Some(&vec![1u64, 2, 3]));
    assert_eq!(m.get_ids("src/main.rs"), None);
}

#[test]
fn test_round_trip_with_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("manifest.json");

    let mut m = Manifest::new();
    m.set_file("src/lib.rs", vec![1, 2]);
    m.set_file("src/main.rs", vec![3]);
    m.save(&path).unwrap();

    let loaded = Manifest::load(&path).unwrap();
    assert_eq!(loaded.get_ids("src/lib.rs"), Some(&vec![1u64, 2]));
    assert_eq!(loaded.get_ids("src/main.rs"), Some(&vec![3u64]));
}

#[test]
fn test_remove_file() {
    let mut m = Manifest::new();
    m.set_file("src/lib.rs", vec![1, 2]);
    m.remove_file("src/lib.rs");
    assert_eq!(m.get_ids("src/lib.rs"), None);
}

#[test]
fn test_overwrite_file_ids() {
    let mut m = Manifest::new();
    m.set_file("src/lib.rs", vec![1, 2]);
    m.set_file("src/lib.rs", vec![10, 20]);
    assert_eq!(m.get_ids("src/lib.rs"), Some(&vec![10u64, 20]));
}
