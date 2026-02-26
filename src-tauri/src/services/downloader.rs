use crate::{inject_system_message, ExportMessage};
use crate::protocol::types::{AppState, SystemLogLevel};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use chrono::{Local, TimeZone};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;

// --- 1. Define Constants for your GGUF Model ---
pub const MODEL_FOLDER: &str = "Qwen3-Blue-Protocol-Translator-JA-KO";
pub const MODEL_FILENAME: &str = "qwen3-1.7b-blueprotocol-ja2ko-q4_k_m.gguf";
// Hugging Face direct download link (using /resolve/main/)
pub const MODEL_URL: &str = "https://huggingface.co/enjay27/Qwen3-Blue-Protocol-Translator-JA-KO/resolve/main/qwen3-1.7b-blueprotocol-ja2ko-q4_k_m.gguf";

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

#[tauri::command]
pub async fn check_model_status(app: tauri::AppHandle) -> Result<ModelStatus, String> {
    // Check exactly one path for the .gguf file
    let model_path = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(MODEL_FOLDER)
        .join(MODEL_FILENAME);

    Ok(ModelStatus {
        exists: model_path.exists(),
        path: model_path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn download_model(app: AppHandle) -> Result<(), String> {
    let model_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(MODEL_FOLDER);

    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let dest_path = model_dir.join(MODEL_FILENAME);

    // Skip if already downloaded
    if dest_path.exists() {
        return Ok(());
    }

    let client = reqwest::Client::new();
    let res = client.get(MODEL_URL).send().await.map_err(|e| e.to_string())?;
    let total_size = res.content_length().unwrap_or(0);

    let mut file = fs::File::create(&dest_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percent = ((downloaded as f32 / total_size as f32) * 100.0) as u8;
            let _ = app.emit("download-progress", ProgressPayload {
                current_file: MODEL_FILENAME.to_string(),
                percent,
                total_percent: percent,
            });
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
    app.path()
        .app_data_dir()
        .expect("Failed to resolve AppData directory")
        .join("models")
        .join(MODEL_FOLDER)
        .join(MODEL_FILENAME)
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

    inject_system_message(&app, SystemLogLevel::Success, "Translator", "Dictionary successfully synchronized.");

    Ok("Dictionary updated and reloaded!".to_string())
}

#[tauri::command]
pub async fn open_app_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    // 1. Resolve the specific AppData/Roaming folder for this app
    let app_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    // 2. CRITICAL: Ensure the directory exists.
    // If Explorer is called on a non-existent path, it defaults to 'Documents'.
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    }

    // 3. Open the folder using the system file explorer
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(app_dir.to_str().unwrap()) // Pass the absolute AppData path
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(app_dir.to_str().unwrap())
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn export_chat_log(app: tauri::AppHandle, logs: Vec<ExportMessage>) -> Result<String, String> {
    // 1. Get the AppData directory
    let app_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    }

    // 2. Create a unique filename based on the current time
    let timestamp_now = Local::now().format("%Y%m%d_%H%M%S");
    let file_path = app_dir.join(format!("chat_export_{}.txt", timestamp_now));

    // 3. Open the file for writing
    let mut file = fs::File::create(&file_path).map_err(|e| e.to_string())?;

    // 4. Write the header
    writeln!(file, "=== BPSR Translator Chat Export ({}) ===", Local::now().format("%Y-%m-%d %H:%M:%S"))
        .map_err(|e| e.to_string())?;
    writeln!(file, "--------------------------------------------------").map_err(|e| e.to_string())?;

    // 5. Format and write each message
    for log in logs {
        // Convert Unix timestamp to readable date/time
        let dt = Local.timestamp_opt(log.timestamp as i64, 0).unwrap();
        let time_str = dt.format("%Y-%m-%d %H:%M:%S");

        // Format translation (if it exists)
        let trans_str = match &log.translated {
            Some(t) => format!(" -> {}", t),
            None => "".to_string(),
        };

        let line = format!("[{}] [{}] {}: {}{}", time_str, log.channel, log.nickname, log.message, trans_str);
        writeln!(file, "{}", line).map_err(|e| e.to_string())?;
    }

    // Return the path so we could theoretically show it to the user
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_browser(app: tauri::AppHandle, url: String) -> Result<(), String> {
    // This tells Windows/macOS to open the URL in Chrome/Edge/Safari
    app.shell().open(url, None).map_err(|e| e.to_string())
}