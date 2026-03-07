use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use futures_util::StreamExt;
use super::{FolderStatus, ProgressPayload};

pub const MODEL_FOLDER: &str = "Qwen3-Blue-Protocol-Translator-JA-KO";
pub const MODEL_FILENAME: &str = "qwen3-4b-blueprotocol-ja2ko-q4_k_m.gguf";
pub const MODEL_URL: &str = "https://huggingface.co/enjay27/Qwen3-Blue-Protocol-Translator-JA-KO/resolve/main/qwen3-4b-blueprotocol-ja2ko-q4_k_m.gguf";

fn get_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(MODEL_FOLDER))
}

pub fn get_model_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("Failed to resolve AppData directory")
        .join("models")
        .join(MODEL_FOLDER)
        .join(MODEL_FILENAME)
}

#[tauri::command]
pub async fn check_model_status(app: tauri::AppHandle) -> Result<FolderStatus, String> {
    // Check exactly one path for the .gguf file
    let model_path = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(MODEL_FOLDER)
        .join(MODEL_FILENAME);

    Ok(FolderStatus {
        exists: model_path.exists(),
        path: model_path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn download_model(app: AppHandle) -> Result<(), String> {
    let model_dir = get_model_dir(&app)?;

    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let dest_path = model_dir.join(MODEL_FILENAME);

    // Skip if already downloaded
    if dest_path.exists() {
        return Ok(());
    }

    let client = reqwest::Client::new();
    let res = client.get(MODEL_URL).send().await.map_err(|e| e.to_string())?;

    // ADD THIS CHECK: Ensure we actually got the file, not an HTML error page
    if !res.status().is_success() {
        return Err(format!("Failed to download model. Server returned: {}", res.status()));
    }

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