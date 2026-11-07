//! Persistence of the site list.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteConfig {
    pub id: u64,
    pub path: PathBuf,
    pub port: u16,
    #[serde(default)]
    pub running: bool,
    /// User-chosen display name; falls back to the folder name.
    #[serde(default)]
    pub custom_name: Option<String>,
}

impl SiteConfig {
    pub fn folder_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

impl SiteConfig {
    pub fn name(&self) -> String {
        self.custom_name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| self.folder_name())
    }
}

pub fn load_grid() -> bool {
    dirs::config_dir()
        .and_then(|d| fs::read_to_string(d.join("http-server").join("view.txt")).ok())
        .is_some_and(|s| s.trim() == "grid")
}

pub fn save_grid(grid: bool) {
    if let Some(d) = dirs::config_dir() {
        let d = d.join("http-server");
        let _ = fs::create_dir_all(&d);
        let _ = fs::write(d.join("view.txt"), if grid { "grid" } else { "list" });
    }
}

fn config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("http-server").join("sites.json"))
}

pub fn load() -> Vec<SiteConfig> {
    config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(sites: &[SiteConfig]) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(sites) {
        let _ = fs::write(path, json);
    }
}
