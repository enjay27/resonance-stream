use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppMetadata {
    pub current_model_version: String,
    pub ignored_app_version: Option<String>,
    pub ignored_model_version: Option<String>,
    pub last_update_check: u64,
}

impl Default for AppMetadata {
    fn default() -> Self {
        Self {
            current_model_version: "0.0.0".to_string(),
            ignored_app_version: None,
            ignored_model_version: None,
            last_update_check: 0,
        }
    }
}

fn get_metadata_path(app: &AppHandle) -> PathBuf {
    let config_dir = app.path().app_config_dir().expect("Could not resolve app config dir");
    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }
    config_dir.join("metadata.json")
}

pub fn load_metadata(app: &AppHandle) -> AppMetadata {
    let path = get_metadata_path(app);

    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(metadata) = serde_json::from_str(&content) {
            return metadata;
        }
    }

    let default_meta = AppMetadata::default();
    save_metadata(app, &default_meta);
    default_meta
}

pub fn save_metadata(app: &AppHandle, metadata: &AppMetadata) {
    let path = get_metadata_path(app);
    if let Ok(json) = serde_json::to_string_pretty(metadata) {
        let _ = fs::write(path, json);
    }
}