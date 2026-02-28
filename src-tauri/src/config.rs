use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use crate::{inject_system_message, AppState, SystemLogLevel};
// Import Manager trait

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub init_done: bool,
    pub use_translation: bool,
    pub compute_mode: String,
    pub compact_mode: bool,
    pub always_on_top: bool,
    pub active_tab: String,
    pub chat_limit: usize,
    pub custom_tab_filters: Vec<String>,
    pub theme: String,
    pub overlay_opacity: f32,
    pub debug_mode: bool,
    pub log_level: String,
    pub tier: String,
    pub archive_chat: bool,
    pub hide_original_in_compact: bool,
    pub network_interface: String,
    pub drag_to_scroll: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            init_done: false,
            use_translation: false,
            compute_mode: "cpu".into(),
            compact_mode: false,
            always_on_top: false,
            active_tab: "전체".to_string(),
            chat_limit: 1000,
            custom_tab_filters: vec!["WORLD".into(), "GUILD".into(), "PARTY".into(), "LOCAL".into()],
            theme: "dark".to_string(),
            overlay_opacity: 0.85,
            debug_mode: false,
            log_level: "info".to_string(),
            tier: "middle".to_string(),
            archive_chat: false,
            hide_original_in_compact: false,
            network_interface: "".to_string(),
            drag_to_scroll: false,
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
pub fn save_config(app: AppHandle, state: State<'_, AppState>, config: AppConfig) {
    let old_config = load_config(app.clone());

    let path = get_config_path(&app);

    if let Ok(json) = serde_json::to_string_pretty(&config) {
        let _ = fs::write(path, json);
    }

    // --- MANAGE THE SNIFFER THREAD (NETWORK ADAPTER CHANGE) ---
    if old_config.network_interface != config.network_interface {
        // Drop the old Sender (Instantly kills the socket and watchdog threads)
        *state.sniffer_tx.lock().unwrap() = None;

        // Restart the sniffer bound to the newly selected interface
        if config.init_done {
            inject_system_message(&app, SystemLogLevel::Info, "Sniffer", "Network adapter changed. Restarting sniffer...");
            let tx = crate::services::sniffer::start_sniffer_worker(app.clone());
            *state.sniffer_tx.lock().unwrap() = Some(tx);
        }
    }

    // --- MANAGE THE AI WORKER THREAD ---
    if !old_config.use_translation && config.use_translation {
        // Turned ON: Start the server and store the Sender
        let model_path = crate::get_model_path(&app);
        let tx = crate::services::translator::start_translator_worker(app.clone(), model_path);
        *state.translator_tx.lock().unwrap() = Some(tx);

    } else if old_config.use_translation && !config.use_translation {
        // Turned OFF: Drop the Sender (Kills the thread and frees VRAM)
        *state.translator_tx.lock().unwrap() = None;
        inject_system_message(&app, SystemLogLevel::Info, "Translator", "AI Translation Disabled. Server stopped and VRAM cleared.");
    }

    // --- MANAGE THE DATA FACTORY THREAD ---
    if !old_config.archive_chat && config.archive_chat {
        // Turned ON: Spawn the I/O thread
        let tx = crate::io::start_data_factory_worker(app.clone());
        *state.data_factory_tx.lock().unwrap() = Some(tx);
        inject_system_message(&app, SystemLogLevel::Info, "DataFactory", "Dataset logging enabled.");
    } else if old_config.archive_chat && !config.archive_chat {
        // Turned OFF: Drop the Sender (Kills the thread)
        *state.data_factory_tx.lock().unwrap() = None;
        inject_system_message(&app, SystemLogLevel::Info, "DataFactory", "Dataset logging disabled.");
    }
}