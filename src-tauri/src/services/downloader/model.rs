use super::{FolderStatus, ProgressPayload};
use futures_util::StreamExt;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use crate::{inject_system_message, SystemLogLevel};

pub const MODEL_FOLDER: &str = "translation-model";
pub const MODEL_FILENAME: &str = "model.gguf";

fn get_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base_models_dir = app
        .path()
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

pub fn calculate_file_hash(path: &std::path::Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;

    // 1. Allocate an 8MB buffer directly on the heap (using vec! prevents stack overflow)
    let mut buffer = vec![0; 8 * 1024 * 1024];
    let mut hasher = Sha256::new();

    loop {
        // 2. Read huge 8MB chunks directly from the disk into our buffer
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(format!("{:x}", hasher.finalize()))
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
pub async fn verify_local_model_hash(app: tauri::AppHandle, expected_hash: String) -> Result<bool, String> {
    let model_path = get_model_path(&app);

    if !model_path.exists() {
        return Ok(false);
    }

    inject_system_message(&app, SystemLogLevel::Info, "Updater", "무결성 검사 중... (Verifying local model...)");

    // Hash the 2.4GB file without freezing the UI thread
    let local_hash = tokio::task::spawn_blocking(move || {
        calculate_file_hash(&model_path)
    }).await.map_err(|e| e.to_string())??;

    let matches = local_hash.eq_ignore_ascii_case(&expected_hash);

    if matches {
        inject_system_message(&app, SystemLogLevel::Success, "Updater", "로컬 모델이 최신 버전과 일치합니다. (Model is already up to date)");
    } else {
        inject_system_message(&app, SystemLogLevel::Warning, "Updater", "모델 해시 불일치. 재다운로드가 필요합니다. (Hash mismatch, download required)");
    }

    Ok(matches)
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    download_url: String,
    version: String,
    expected_hash: String, // Takes the hash from the UI
) -> Result<(), String> {
    let model_dir = get_model_dir(&app)?;
    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let final_path = model_dir.join(MODEL_FILENAME);
    let temp_path = model_dir.join(format!("{}.tmp", MODEL_FILENAME));

    // ==========================================================
    // 1. FAST-PATH: Check if the perfect file already exists!
    // ==========================================================
    if final_path.exists() {
        inject_system_message(&app, SystemLogLevel::Info, "Model", "Checking existing model integrity before downloading...");

        let local_hash = tokio::task::spawn_blocking({
            let fp = final_path.clone();
            move || calculate_file_hash(&fp)
        }).await.map_err(|e| e.to_string())??;

        println!("Local hash: {}", local_hash);

        if local_hash.eq_ignore_ascii_case(&expected_hash) {
            inject_system_message(&app, SystemLogLevel::Success, "Model", "Existing model is a perfect match! Skipping download.");

            // Emit a fake 100% progress so the UI gracefully moves forward
            let _ = app.emit("download-progress", ProgressPayload {
                current_file: "로컬 AI 모델 확인 완료 (Skipped download)".to_string(),
                percent: 100,
                total_percent: 100,
            });

            // Commit new version metadata
            let mut metadata = crate::config::load_metadata(&app);
            metadata.current_model_version = version;
            crate::config::save_metadata(&app, &metadata);

            return Ok(());
        } else {
            inject_system_message(&app, SystemLogLevel::Warning, "Model", "Existing model is outdated or corrupted. Starting fresh download.");
        }
    }

    // ==========================================================
    // 2. DOWNLOAD: If missing or corrupted, download to .tmp
    // ==========================================================
    inject_system_message(&app, SystemLogLevel::Info, "Model", format!("Download Model version {}", version));

    let client = reqwest::Client::new();
    let res = client.get(&download_url).send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to download model. Server returned: {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(0);
    let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
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

    // ==========================================================
    // 3. VERIFY: Download finished, verify the TMP file hash!
    // ==========================================================
    let _ = app.emit("download-progress", ProgressPayload {
        current_file: "파일 무결성 검증 중... (Verifying...)".into(),
        percent: 100,
        total_percent: 100,
    });

    let downloaded_hash = tokio::task::spawn_blocking({
        let tp = temp_path.clone();
        move || calculate_file_hash(&tp)
    }).await.map_err(|e| e.to_string())??;

    if downloaded_hash.eq_ignore_ascii_case(&expected_hash) {
        // Safe overwrite: Replace old model with the verified temp file
        fs::rename(&temp_path, &final_path).map_err(|e| e.to_string())?;

        let mut metadata = crate::config::load_metadata(&app);
        metadata.current_model_version = version;
        crate::config::save_metadata(&app, &metadata);

        inject_system_message(&app, SystemLogLevel::Success, "Model", "Model verified and installed successfully.");
        Ok(())
    } else {
        // Delete the corrupted temp file
        let _ = fs::remove_file(&temp_path);
        let err_msg = format!("Hash mismatch! Expected {}, got {}. Download discarded.", expected_hash, downloaded_hash);
        inject_system_message(&app, SystemLogLevel::Error, "Model", &err_msg);
        Err("File corruption detected. Please try downloading again.".to_string())
    }
}