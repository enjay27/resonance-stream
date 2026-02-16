use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager}; // Import Manager trait

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub compact_mode: bool,
    pub always_on_top: bool,
    pub active_tab: String,
    pub chat_limit: usize,
    pub custom_tab_filters: Vec<String>,
    pub theme: String,
    pub overlay_opacity: f32,
    pub show_system_tab: bool,
    pub is_debug: bool,
    pub tier: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            compact_mode: false,
            always_on_top: false,
            active_tab: "전체".to_string(),
            chat_limit: 1000,
            custom_tab_filters: vec!["WORLD".into(), "GUILD".into(), "PARTY".into(), "LOCAL".into()],
            theme: "dark".to_string(),
            overlay_opacity: 0.85,
            show_system_tab: false,
            is_debug: false,
            tier: "middle".to_string(),
        }
    }
}

// Helper to get the correct path:
// Windows: C:\Users\Name\AppData\Roaming\com.your.identifier\config.json
// Mac: /Users/Name/Library/Application Support/com.your.identifier/config.json
fn get_config_path(app: &AppHandle) -> PathBuf {
    let config_dir = app.path().app_config_dir().expect("Could not resolve app config dir");

    // Ensure the directory exists (e.g., create 'com.bpsr.translator' folder)
    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }

    config_dir.join("config.json")
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> AppConfig {
    let path = get_config_path(&app);

    if !path.exists() {
        // Create default if missing
        let default_config = AppConfig::default();
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            let _ = fs::write(&path, json);
        }
        return default_config;
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: AppConfig) {
    let path = get_config_path(&app);
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        let _ = fs::write(path, json);
    }
}