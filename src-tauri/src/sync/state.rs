//! `sync_state.json`: last-synced version/sha per pack and file.
use std::{collections::HashMap, fs, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};

use super::plan::Base;

#[derive(Default, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    packs: HashMap<String, HashMap<String, Base>>,
    /// Character display names by id (the file name without extension), read
    /// from `.player` files so remote-only entries can still show a name.
    #[serde(default)]
    names: HashMap<String, String>,
}

impl SyncState {
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp: PathBuf = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&tmp, data).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn get(&self, pack: &str, name: &str) -> Option<&Base> {
        self.packs.get(pack)?.get(name)
    }

    pub fn set(&mut self, pack: &str, name: &str, base: Base) {
        self.packs.entry(pack.to_string()).or_default().insert(name.to_string(), base);
    }

    pub fn name(&self, id: &str) -> Option<&String> {
        self.names.get(id)
    }

    pub fn set_name(&mut self, id: &str, name: String) {
        self.names.insert(id.to_string(), name);
    }

    pub fn clear(&mut self, pack: &str, name: &str) {
        if let Some(p) = self.packs.get_mut(pack) {
            p.remove(name);
        }
    }
}
