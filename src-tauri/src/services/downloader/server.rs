use std::fs;
use std::io::Write;
use tauri::{AppHandle, Emitter, Manager};
use futures_util::StreamExt;
use super::{FolderStatus, ProgressPayload};

pub const AI_SERVER_FOLDER: &str = "ai-server";
pub const AI_SERVER_ZIP_URL: &str = "https://github.com/enjay27/resonance-stream/releases/download/v0.2.0/llama-b8157-bin-win-vulkan-x64.zip";
pub const AI_SERVER_FILENAME: &str = "llama-server.exe";

#[tauri::command]
pub async fn check_ai_server_status(app: tauri::AppHandle) -> Result<FolderStatus, String> {
    // Check exactly one path for the .gguf file
    let model_path = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("bin")
        .join(AI_SERVER_FOLDER)
        .join(AI_SERVER_FILENAME);

    Ok(FolderStatus {
        exists: model_path.exists(),
        path: model_path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn download_ai_server(app: AppHandle) -> Result<(), String> {
    let ai_server_dir = app.path().app_data_dir().unwrap()
        .join("bin")
        .join(AI_SERVER_FOLDER);
    fs::create_dir_all(&ai_server_dir).map_err(|e| e.to_string())?;

    let server_exe = ai_server_dir.join("llama-server.exe");

    // Skip if already downloaded and extracted
    if server_exe.exists() {
        return Ok(());
    }

    let zip_path = ai_server_dir.join("server_temp.zip");

    // 1. Download the ZIP file (Streaming)
    let client = reqwest::Client::new();
    let res = client.get(AI_SERVER_ZIP_URL).send().await.map_err(|e| e.to_string())?;

    // ADD THIS CHECK: Ensure we didn't hit a 404 on GitHub
    if !res.status().is_success() {
        return Err(format!("Failed to download AI server. Server returned: {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(0);

    let mut file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percent = ((downloaded as f32 / total_size as f32) * 100.0) as u8;
            let _ = app.emit("download-progress", ProgressPayload {
                current_file: "AI 엔진 다운로드 중...".to_string(),
                percent,
                total_percent: percent,
            });
        }
    }

    // 2. Extract the ZIP file
    let _ = app.emit("download-progress", ProgressPayload {
        current_file: "압축 해제 중...".to_string(),
        percent: 100,
        total_percent: 100,
    });

    let zip_file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => ai_server_dir.join(path.file_name().unwrap_or(path.as_os_str())), // Flattens the folder structure
            None => continue,
        };

        if file.name().ends_with('/') {
            continue; // Skip directories
        }

        let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
    }

    // 3. Clean up the temp zip file
    let _ = fs::remove_file(zip_path);

    Ok(())
}
