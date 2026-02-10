use std::io::Write;
use std::path::PathBuf;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use tauri::path::BaseDirectory;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// Constants for our recommended models
pub const URL_QWEN_1_7B: &str = "https://huggingface.co/Qwen/Qwen3-1.7B-Instruct-GGUF/resolve/main/qwen3-1.7b-instruct-q4_k_m.gguf";
pub const FILENAME_1_7B: &str = "qwen3-1.7b-instruct.gguf";

pub const URL_QWEN_0_5B: &str = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";
pub const FILENAME_0_5B: &str = "qwen2.5-0.5b-instruct.gguf";

#[derive(serde::Serialize)]
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

// Helper: Get the path to the 'models' folder inside AppData
fn get_models_dir(app: &AppHandle) -> PathBuf {
    let app_data = app.path().resolve("", BaseDirectory::AppData).unwrap();
    let models_dir = app_data.join("models");

    // Ensure directory exists
    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir).expect("Failed to create models dir");
    }
    models_dir
}

#[tauri::command]
fn check_model_status(app: AppHandle, filename: String) -> () {
    println!("check_model_status called with {}", filename);
    // let dir = get_models_dir(&app);
    // let file_path = dir.join(&filename);
    //
    // ModelStatus {
    //     exists: file_path.exists(),
    //     path: file_path.to_string_lossy().to_string(),
    // }
}

#[tauri::command]
async fn download_model(app: AppHandle, url: String) -> () {
    println!("download_model called with {}", url);
    // let dir = get_models_dir(&app);
    // let file_path = dir.join(&filename);
    //
    // // 1. Setup Request
    // let client = reqwest::Client::new();
    // let res = client.get(&url).send().await.map_err(|e| e.to_string())?;
    //
    // let total_size = res.content_length().unwrap_or(0);
    //
    // // 2. Setup File Writer
    // let mut file = std::fs::File::create(&file_path).map_err(|e| e.to_string())?;
    // let mut stream = res.bytes_stream();
    // let mut downloaded: u64 = 0;
    //
    // // 3. Stream Loop
    // while let Some(item) = stream.next().await {
    //     let chunk = item.map_err(|e| e.to_string())?;
    //     file.write_all(&chunk).map_err(|e| e.to_string())?;
    //
    //     downloaded += chunk.len() as u64;
    //
    //     // 4. Emit Progress Event (Optimize: Don't emit every single chunk, maybe every 1%)
    //     if total_size > 0 {
    //         let percent = ((downloaded as f64 / total_size as f64) * 100.0) as u8;
    //         app.emit("download-progress", ProgressPayload {
    //             current: downloaded,
    //             total: total_size,
    //             percent,
    //         }).unwrap();
    //     }
    // }
    //
    // Ok(file_path.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, check_model_status, download_model])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}