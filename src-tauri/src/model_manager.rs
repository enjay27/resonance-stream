use futures_util::StreamExt;
use serde::Serialize;
use std::io::Write;
use tauri::{AppHandle, Emitter, Manager};
use crate::inject_system_message;
use crate::sniffer::AppState;

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

struct ModelFile {
    name: &'static str,
    url: &'static str,
}

const MODEL_FILES: [ModelFile; 4] = [
    ModelFile {
        name: "model.bin",
        url: "https://huggingface.co/JustFrederik/nllb-200-distilled-1.3B-ct2-int8/resolve/main/model.bin",
    },
    ModelFile {
        name: "config.json",
        url: "https://huggingface.co/JustFrederik/nllb-200-distilled-1.3B-ct2-int8/resolve/main/config.json",
    },
    ModelFile {
        name: "shared_vocabulary.txt",
        url: "https://huggingface.co/JustFrederik/nllb-200-distilled-1.3B-ct2-int8/resolve/main/shared_vocabulary.txt",
    },
    ModelFile {
        name: "tokenizer.model", // We save it as tokenizer.model locally for the Python script
        url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/sentencepiece.bpe.model",
    },
];

#[tauri::command]
pub async fn check_model_status(app: AppHandle) -> Result<ModelStatus, String> {
    // Update path to separate the 1.3B model from your old 600M files
    let model_dir = app.path().app_data_dir().unwrap().join("models/nllb_1.3B_int8");
    let all_exist = MODEL_FILES.iter().all(|f| model_dir.join(f.name).exists());

    Ok(ModelStatus {
        exists: all_exist,
        path: model_dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn download_model(app: AppHandle) -> Result<(), String> {
    let model_dir = app.path().app_data_dir().unwrap().join("models/nllb_1.3B_int8");
    std::fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let client = reqwest::Client::new();
    let total_files = MODEL_FILES.len() as f32;

    for (idx, file_info) in MODEL_FILES.iter().enumerate() {
        let dest_path = model_dir.join(file_info.name);

        // Skip if individual file exists (basic resumption)
        if dest_path.exists() { continue; }

        let res = client.get(file_info.url).send().await.map_err(|e| e.to_string())?;
        let total_size = res.content_length().unwrap_or(0);

        let mut file = std::fs::File::create(&dest_path).map_err(|e| e.to_string())?;
        let mut downloaded: u64 = 0;
        let mut stream = res.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| e.to_string())?;
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;

            if total_size > 0 {
                let file_percent = ((downloaded as f32 / total_size as f32) * 100.0) as u8;
                let total_percent = (((idx as f32 / total_files) * 100.0) + (file_percent as f32 / total_files)) as u8;

                let _ = app.emit("download-progress", ProgressPayload {
                    current_file: file_info.name.to_string(),
                    percent: file_percent,
                    total_percent,
                });
            }
        }
    }

    Ok(())
}

pub fn get_model_path(app: &AppHandle) -> String {
    app.path()
        .app_data_dir()
        .expect("Failed to resolve AppData directory")
        .join("models/nllb_1.3B_int8")
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

    inject_system_message(&app, "[Translator] Dictionary downloaded and saved");

    // 5. Notify Python Sidecar via Stdin
    let mut tx_guard = state.tx.lock().unwrap();
    if let Some(child) = tx_guard.as_mut() {
        let msg = serde_json::json!({ "cmd": "reload" }).to_string() + "\n";

        // Use the child's write method
        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;
        inject_system_message(&app, "[Sidecar] Dictionary reload command sent.");
    } else {
        return Err("Translator is not running".into());
    }

    inject_system_message(&app, "[Translator] Dictionary successfully reloaded");

    Ok("Dictionary updated and reloaded!".to_string())
}
