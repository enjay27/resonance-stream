use tauri::AppHandle;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct VersionInfo {
    pub latest_version: String,
    pub download_url: String,
    pub release_notes: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RemoteVersions {
    pub app: VersionInfo,
    pub model: VersionInfo,
}

#[derive(Serialize)]
pub struct UpdateCheckResult {
    pub app_update_available: bool,
    pub model_update_available: bool,
    pub remote_data: RemoteVersions,
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    // 1. Paste your PERMANENT RAW GIST URL here
    let gist_url = "https://gist.githubusercontent.com/enjay27/YOUR_GIST_ID/raw/bpsr_versions.json";

    let client = reqwest::Client::new();
    let remote_data: RemoteVersions = client.get(gist_url)
        .send().await.map_err(|e| format!("Network error: {}", e))?
        .json().await.map_err(|e| format!("JSON parsing error: {}", e))?;

    // 2. Load the System Metadata (where we save the ignored versions)
    // Assuming you set up src/config/metadata.rs and exported it in src/config/mod.rs
    let metadata = crate::config::load_metadata(&app);

    // Tauri automatically pulls the current app version from tauri.conf.json!
    let current_app_version = app.package_info().version.to_string();

    // 3. Evaluate App Update
    let mut app_update_available = remote_data.app.latest_version != current_app_version;
    if let Some(ignored) = &metadata.ignored_app_version {
        if ignored == &remote_data.app.latest_version {
            app_update_available = false;
        }
    }

    // 4. Evaluate Model Update
    let mut model_update_available = remote_data.model.latest_version != metadata.current_model_version;
    if let Some(ignored) = &metadata.ignored_model_version {
        if ignored == &remote_data.model.latest_version {
            model_update_available = false;
        }
    }

    Ok(UpdateCheckResult {
        app_update_available,
        model_update_available,
        remote_data,
    })
}

#[tauri::command]
pub fn ignore_update(app: AppHandle, target: String, version: String) {
    let mut metadata = crate::config::load_metadata(&app);

    if target == "app" {
        metadata.ignored_app_version = Some(version);
    } else if target == "model" {
        metadata.ignored_model_version = Some(version);
    }

    crate::config::save_metadata(&app, &metadata);
}