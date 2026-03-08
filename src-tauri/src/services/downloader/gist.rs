use tauri::{AppHandle, Manager};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use crate::{inject_system_message, AppState};
use crate::protocol::types::SystemLogLevel;

const METADATA_URL: &str = "https://gist.githubusercontent.com/enjay27/4066e54b9c2ac6c923bf967e6d9a06c5/raw/8a88850437be4331c9c12b79ef445350fd33543f/metadata.json";
const DICT_URL: &str = "https://gist.githubusercontent.com/enjay27/4066e54b9c2ac6c923bf967e6d9a06c5/raw/4bc13d890e464cc4849b91c6f1c6d1da5a983255/custom_dict.json";

// --- 1. Structs matching your new unified Gist JSON ---
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct VersionInfo {
    pub latest_version: String,
    pub download_url: String,
    pub release_notes: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RemoteDictionary {
    pub version: String,
    pub updated_at: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GistMetadata {
    pub app: VersionInfo,
    pub model: VersionInfo,
    pub dictionary: RemoteDictionary,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateCheckResult {
    pub app_update_available: bool,
    pub model_update_available: bool,
    pub dict_update_available: bool,
    pub remote_data: GistMetadata,
}

// --- 2. The Single Unified Fetch Command ---
#[tauri::command]
pub async fn check_all_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    // Paste your PERMANENT RAW URL here

    let client = reqwest::Client::new();
    let remote_data: GistMetadata = client.get(METADATA_URL)
        .send().await.map_err(|e| format!("Network error: {}", e))?
        .json().await.map_err(|e| format!("JSON parsing error: {}", e))?;

    let metadata = crate::config::load_metadata(&app);
    let current_app_version = app.package_info().version.to_string();

    // App Check
    let mut app_update_available = remote_data.app.latest_version != current_app_version;
    if let Some(ignored) = &metadata.ignored_app_version {
        if ignored == &remote_data.app.latest_version { app_update_available = false; }
    }

    // Model Check
    let mut model_update_available = remote_data.model.latest_version != metadata.current_model_version;
    if let Some(ignored) = &metadata.ignored_model_version {
        if ignored == &remote_data.model.latest_version { model_update_available = false; }
    }

    // Dictionary Check
    let dict_update_available = remote_data.dictionary.version != metadata.current_dict_version;

    Ok(UpdateCheckResult {
        app_update_available,
        model_update_available,
        dict_update_available,
        remote_data,
    })
}

#[tauri::command]
pub async fn sync_dictionary(app: AppHandle) -> Result<String, String> {
    // 1. Resolve Local Path: %APPDATA%/your.bundle.id/custom_dict.json
    let dict_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("custom_dict.json");

    // 2. Fetch from Remote
    let client = reqwest::Client::new();
    let response = client.get(DICT_URL).send().await.map_err(|e| e.to_string())?;
    let json_content = response.text().await.map_err(|e| e.to_string())?;

    // Validate JSON before saving
    if serde_json::from_str::<serde_json::Value>(&json_content).is_err() {
        return Err("Invalid JSON received from Gist".to_string());
    }

    // 3. Save Locally
    fs::create_dir_all(dict_path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&dict_path, &json_content).map_err(|e| e.to_string())?;

    inject_system_message(&app, SystemLogLevel::Success, "ModelManager", "Dictionary saved to AppData.");

    inject_system_message(&app, SystemLogLevel::Success, "Translator", "Dictionary successfully synchronized.");

    Ok("Dictionary updated and reloaded!".to_string())
}

#[tauri::command]
pub fn get_dict_version(app: tauri::AppHandle) -> String {
    let metadata = crate::config::load_metadata(&app);
    metadata.current_dict_version
}

#[tauri::command]
pub fn get_local_dictionary(app: tauri::AppHandle) -> Result<String, String> {
    let dict_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("custom_dict.json");

    if !dict_path.exists() {
        return Ok("{}".to_string());
    }

    std::fs::read_to_string(&dict_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_local_dictionary(app: tauri::AppHandle, content: String) -> Result<(), String> {
    let dict_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("custom_dict.json");

    std::fs::write(&dict_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ignore_update(app: AppHandle, target: String, version: String) {
    let mut metadata = crate::config::load_metadata(&app);
    if target == "app" { metadata.ignored_app_version = Some(version); }
    else if target == "model" { metadata.ignored_model_version = Some(version); }
    crate::config::save_metadata(&app, &metadata);
}