use tauri::{AppHandle, Manager, Emitter};
use tauri::path::BaseDirectory;
use std::path::PathBuf;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

// --- CONFIGURATION ---
const MODEL_URL: &str = "https://huggingface.co/lm-kit/qwen-3-0.6b-instruct-gguf/resolve/main/Qwen3-0.6B-Q4_K_M.gguf";
const MODEL_FILENAME: &str = "Qwen3-0.6B-Q4_K_M.gguf";

#[derive(serde::Serialize, Clone)]
pub struct ModelStatus {
    pub exists: bool,
    pub path: String,
}

#[derive(serde::Serialize, Clone)]
pub struct ProgressPayload {
    pub current: u64,
    pub total: u64,
    pub percent: u8,
}

// Helper: Get Model Path
pub fn get_model_path(app: &AppHandle) -> PathBuf {
    let app_data = app.path().resolve("", BaseDirectory::AppData).unwrap();
    let models_dir = app_data.join("models");
    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir).expect("Failed to create models dir");
    }
    models_dir.join(MODEL_FILENAME)
}

#[tauri::command]
pub fn check_model_status(app: AppHandle) -> ModelStatus {
    let file_path = get_model_path(&app);
    ModelStatus {
        exists: file_path.exists(),
        path: file_path.to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub async fn download_model(app: AppHandle) -> Result<String, String> {
    let file_path = get_model_path(&app);

    // 1. Setup Client
    let client = reqwest::Client::new();
    let res = client
        .get(MODEL_URL)
        .header("User-Agent", "BPSR-Translator/1.0")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Download failed: {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(0);

    // 2. Create File
    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    // 3. Stream
    let mut stream = res.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = 0;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;

        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percent = ((downloaded as f64 / total_size as f64) * 100.0) as u8;
            if percent > last_emit {
                last_emit = percent;
                app.emit("download-progress", ProgressPayload {
                    current: downloaded,
                    total: total_size,
                    percent,
                }).unwrap_or_else(|_| {});
            }
        }
    }

    Ok(file_path.to_string_lossy().to_string())
}