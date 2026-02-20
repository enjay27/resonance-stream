use std::fs;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::Write;
use tauri::{AppHandle, Emitter, Manager};
use tauri::path::BaseDirectory;
use crate::inject_system_message;
use crate::sniffer::{AppState, SystemLogLevel};

#[derive(Serialize, Clone)]
pub struct ModelStatus {
    pub exists: bool,
    pub path: String,
}

#[derive(Serialize, Clone)]
struct ProgressPayload {
    pub current_file: String,
    pub percent: u8,
    pub total_percent: u8,
}

#[derive(Deserialize, Serialize, Clone)]
struct ModelFile {
    name: String,
    url: String,
}

#[derive(Deserialize, Clone)]
struct ModelManifest {
    model_id: String,
    files: Vec<ModelFile>,
}

fn load_manifest(app: &tauri::AppHandle) -> Result<ModelManifest, String> {
    // 1. DEVELOPMENT: Embed the file directly into the binary during 'cargo tauri dev'
    #[cfg(debug_assertions)]
    {
        // This path is relative to 'src-tauri/src/model_manager.rs'
        let json_data = include_str!("../resources/models.json");
        serde_json::from_str(json_data).map_err(|e| format!("Dev JSON parse error: {}", e))
    }

    // 2. PRODUCTION: Use the dynamic PathResolver for the bundled resource
    #[cfg(not(debug_assertions))]
    {
        use tauri::path::BaseDirectory;

        // Fix: Match the structure in tauri.conf.json ("resources/models.json")
        let resource_path = app.path()
            .resolve("resources/models.json", BaseDirectory::Resource)
            .map_err(|e| format!("Resource resolution failed: {}", e))?;

        if !resource_path.exists() {
            // Fallback: Check the same directory as the EXE (useful for local builds)
            let exe_path = std::env::current_exe().unwrap().parent().unwrap().join("models.json");
            if exe_path.exists() {
                let json_data = std::fs::read_to_string(&exe_path).map_err(|e| e.to_string())?;
                return serde_json::from_str(&json_data).map_err(|e| e.to_string());
            }
            return Err(format!("models.json not found. Checked: {:?}", resource_path));
        }

        let json_data = std::fs::read_to_string(&resource_path)
            .map_err(|e| format!("Failed to read models.json: {}", e))?;

        serde_json::from_str(&json_data).map_err(|e| format!("Prod JSON parse error: {}", e))
    }
}

#[tauri::command]
pub async fn check_model_status(app: tauri::AppHandle) -> Result<ModelStatus, String> {
    // 1. Load the manifest using the same hybrid logic as load_manifest
    let manifest = load_manifest(&app)?;

    // 2. Resolve the model directory based on the manifest's model_id
    let model_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(format!("models/{}", manifest.model_id));

    // 3. Verify if all required files exist in that directory
    let all_exist = manifest.files.iter().all(|f| model_dir.join(&f.name).exists());

    Ok(ModelStatus {
        exists: all_exist,
        path: model_dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn download_model(app: AppHandle) -> Result<(), String> {
    let manifest = load_manifest(&app)?;
    let model_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(format!("models/{}", manifest.model_id));

    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let client = reqwest::Client::new();

    for (idx, file_info) in manifest.files.iter().enumerate() {
        let dest_path = model_dir.join(&file_info.name);
        if dest_path.exists() {
            continue;
        }

        let res = client.get(&file_info.url).send().await.map_err(|e| e.to_string())?;
        let total_size = res.content_length().unwrap_or(0);

        let mut file = fs::File::create(&dest_path).map_err(|e| e.to_string())?;
        let mut downloaded: u64 = 0;
        let mut stream = res.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| e.to_string())?;
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;

            if file_info.name == "model.bin" && total_size > 0 {
                let percent = ((downloaded as f32 / total_size as f32) * 100.0) as u8;
                let _ = app.emit("download-progress", ProgressPayload {
                    current_file: file_info.name.clone(),
                    percent, // Local file percent
                    total_percent: percent, // We use this as the primary UI driver
                });
            }
        }
    }

    let _ = app.emit("download-progress", ProgressPayload {
        current_file: "완료".into(),
        percent: 100,
        total_percent: 100,
    });

    Ok(())
}

pub fn get_model_path(app: &tauri::AppHandle) -> String {
    // We can unwrap safely here if we know load_manifest is solid
    let manifest = load_manifest(app).expect("Failed to load manifest for path resolution");

    app.path()
        .app_data_dir()
        .expect("Failed to resolve AppData directory")
        .join(format!("models/{}", manifest.model_id))
        .to_string_lossy()
        .into_owned()
}

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

    // 5. Notify Python Sidecar via Stdin
    let mut tx_guard = state.tx.lock().unwrap();
    if let Some(child) = tx_guard.as_mut() {
        let msg = serde_json::json!({ "cmd": "reload" }).to_string() + "\n";

        // Use the child's write method
        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;
        inject_system_message(&app, SystemLogLevel::Info, "Sidecar", "Sent reload command via Stdin.");
    } else {
        return Err("Translator is not running".into());
    }

    inject_system_message(&app, SystemLogLevel::Success, "Translator", "Dictionary successfully synchronized.");

    Ok("Dictionary updated and reloaded!".to_string())
}
