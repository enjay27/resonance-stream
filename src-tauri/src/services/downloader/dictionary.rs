use crate::inject_system_message;
use crate::protocol::types::{AppState, SystemLogLevel};
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn check_dict_update(app: tauri::AppHandle) -> Result<bool, String> {
    let gist_url = "https://gist.githubusercontent.com/enjay27/487b588d38cd6bd514bc2be3d2db8270/raw/bp_dictionary.json";
    let local_path = app.path().app_data_dir().unwrap().join("custom_dict.json");

    // 1. Get the remote version (or last-modified header)
    let client = reqwest::Client::new();
    let response = client.get(gist_url).send().await.map_err(|e| e.to_string())?;
    let remote_json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

    let remote_version = remote_json["version"].as_str().unwrap_or("0.0.0");

    // 2. Read the local version
    if let Ok(local_content) = std::fs::read_to_string(local_path) {
        let local_json: serde_json::Value = serde_json::from_str(&local_content).unwrap_or_default();
        let local_version = local_json["version"].as_str().unwrap_or("0.0.0");

        // 3. Compare (e.g., "1.0.5" vs "1.0.4")
        return Ok(remote_version != local_version);
    }

    Ok(true)
}

#[tauri::command]
pub async fn sync_dictionary(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<String, String> {
    // 1. Define your Raw Gist URL
    let url = "https://gist.githubusercontent.com/enjay27/487b588d38cd6bd514bc2be3d2db8270/raw/bp_dictionary.json";

    // 2. Resolve Local Path: %APPDATA%/your.bundle.id/custom_dict.json
    let dict_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("custom_dict.json");

    // 3. Fetch from Remote
    let client = reqwest::Client::new();
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    let json_content = response.text().await.map_err(|e| e.to_string())?;

    // Validate JSON before saving
    if serde_json::from_str::<serde_json::Value>(&json_content).is_err() {
        return Err("Invalid JSON received from Gist".to_string());
    }

    // 4. Save Locally
    std::fs::create_dir_all(dict_path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&dict_path, &json_content).map_err(|e| e.to_string())?;

    inject_system_message(&app, SystemLogLevel::Success, "ModelManager", "Dictionary saved to AppData.");

    inject_system_message(&app, SystemLogLevel::Success, "Translator", "Dictionary successfully synchronized.");

    Ok("Dictionary updated and reloaded!".to_string())
}