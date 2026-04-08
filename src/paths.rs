use std::path::PathBuf;

pub fn default_db_path() -> PathBuf {
    data_dir().join("entic").join("chunks.db")
}

pub fn default_manifest_path() -> PathBuf {
    data_dir().join("entic").join("manifest.json")
}

fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(p);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share");
    }
    PathBuf::from(".")
}
