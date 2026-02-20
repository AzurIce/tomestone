use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub game_install_dir: Option<PathBuf>,
}

pub fn config_path() -> PathBuf {
    data_root().join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

pub fn data_root() -> PathBuf {
    let root = if cfg!(debug_assertions) {
        PathBuf::from("./.tomestone")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".tomestone")
    };
    let _ = std::fs::create_dir_all(&root);
    root
}

pub fn data_subdir(name: &str) -> PathBuf {
    let dir = data_root().join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn glamours_dir() -> PathBuf {
    data_subdir("glamours")
}

pub fn schema_dir() -> PathBuf {
    data_subdir("schema")
}
