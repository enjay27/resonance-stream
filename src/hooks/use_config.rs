use crate::tauri_bridge::invoke;
use crate::ui_types::{AppConfig, SystemLogLevel};
use crate::utils::add_system_log;

pub async fn save_app_config(config: AppConfig) {
    if let Ok(args) = serde_wasm_bindgen::to_value(&serde_json::json!({ "config": config })) {
        // The backend keeps the setting for this run even when the file cannot be written; it
        // answers with why, and the user is told it will not survive a restart.
        if let Err(e) = invoke("save_config", args).await {
            let why = e.as_string().unwrap_or_else(|| "unknown error".to_string());
            add_system_log(SystemLogLevel::Error, "Settings", &why);
        }
    }
}
