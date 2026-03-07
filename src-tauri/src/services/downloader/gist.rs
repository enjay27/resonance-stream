use tauri::{AppHandle, Manager};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use crate::inject_system_message;
use crate::protocol::types::SystemLogLevel;

const GIST_URL: &str = "https://gist.githubusercontent.com/enjay27/ae9c1e66f903c9ea74442753bcba0df2/raw/170f6628b7504a84e0c584556ec138034353039e/bpsr_meta.json";

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
    pub data: HashMap<String, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GistMetadata {
    pub app: VersionInfo,
    pub model: VersionInfo,
    pub dictionary: RemoteDictionary,
}

#[derive(Serialize)]
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
    let remote_data: GistMetadata = client.get(GIST_URL)
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

// --- 3. Applying the Dictionary Update (No HTTP request needed!) ---
#[tauri::command]
pub fn apply_dictionary_update(app: AppHandle, new_dict: RemoteDictionary) -> Result<String, String> {
    let dict_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("custom_dict.json");

    // Re-wrap the dictionary so it perfectly matches what processor.rs expects
    let json_wrapper = serde_json::json!({
        "version": new_dict.version,
        "data": new_dict.data
    });

    // Save the file
    if let Some(parent) = dict_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&dict_path, json_wrapper.to_string()).map_err(|e| e.to_string())?;

    // Update Local Metadata Tracker
    let mut metadata = crate::config::load_metadata(&app);
    metadata.current_dict_version = new_dict.version;
    crate::config::save_metadata(&app, &metadata);

    inject_system_message(&app, SystemLogLevel::Success, "Translator", "Dictionary successfully synchronized.");
    Ok("Dictionary updated and reloaded!".to_string())
}

#[tauri::command]
pub fn ignore_update(app: AppHandle, target: String, version: String) {
    let mut metadata = crate::config::load_metadata(&app);
    if target == "app" { metadata.ignored_app_version = Some(version); }
    else if target == "model" { metadata.ignored_model_version = Some(version); }
    crate::config::save_metadata(&app, &metadata);
}