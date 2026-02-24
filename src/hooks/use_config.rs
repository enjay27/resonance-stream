use crate::tauri_bridge::invoke;
use crate::types::AppConfig;

pub async fn save_app_config(config: AppConfig) {
    if let Ok(args) = serde_wasm_bindgen::to_value(&serde_json::json!({ "config": config })) {
        let _ = invoke("save_config", args).await;
    }
}