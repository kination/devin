use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub entries: HashMap<String, Vec<u64>>,
}

impl Manifest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let m = serde_json::from_str(&content)?;
        Ok(m)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn set_file(&mut self, file: &str, ids: Vec<u64>) {
        self.entries.insert(file.to_string(), ids);
    }

    pub fn remove_file(&mut self, file: &str) {
        self.entries.remove(file);
    }

    pub fn get_ids(&self, file: &str) -> Option<&Vec<u64>> {
        self.entries.get(file)
    }
}
