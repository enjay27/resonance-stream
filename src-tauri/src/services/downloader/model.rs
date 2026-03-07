use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use futures_util::StreamExt;
use super::{FolderStatus, ProgressPayload};

pub const MODEL_FOLDER: &str = "translation-model";
pub const MODEL_FILENAME: &str = "model.gguf";

fn get_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base_models_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models");

    let new_dir = base_models_dir.join(MODEL_FOLDER); // "translation-model"
    let old_dir = base_models_dir.join("Qwen3-Blue-Protocol-Translator-JA-KO");

    // MIGRATION: If the old folder exists but the new one doesn't, rename it!
    if old_dir.exists() && !new_dir.exists() {
        let _ = fs::rename(&old_dir, &new_dir);

        // Also rename the specific .gguf file to the generic model.gguf
        let old_file = new_dir.join("qwen3-4b-blueprotocol-ja2ko-q4_k_m.gguf");
        let new_file = new_dir.join(MODEL_FILENAME);
        if old_file.exists() {
            let _ = fs::rename(&old_file, &new_file);
        }
    }

    Ok(new_dir)
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
    let model_path = get_model_path(&app);

    Ok(FolderStatus {
        exists: model_path.exists(),
        path: model_path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn download_model(app: AppHandle, download_url: String, version: String) -> Result<(), String> {
    let model_dir = get_model_dir(&app)?;

    // 1. Cleanup: If the folder exists, delete it first to remove old 4GB model files
    if model_dir.exists() {
        let _ = fs::remove_dir_all(&model_dir);
    }
    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let dest_path = model_dir.join(MODEL_FILENAME);

    // 2. Download from the dynamic URL provided by the Gist
    let client = reqwest::Client::new();
    let res = client.get(&download_url).send().await.map_err(|e| e.to_string())?;

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
                current_file: "AI 모델 다운로드 중...".to_string(),
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

    // 3. Commit the new version to metadata so the update checker knows we have it
    let mut metadata = crate::config::load_metadata(&app);
    metadata.current_model_version = version;
    crate::config::save_metadata(&app, &metadata);

    Ok(())
}