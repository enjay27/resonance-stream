use std::env;
use std::fs;
use std::io::Write;
use std::process::Command;
use tauri::{AppHandle, Emitter};
use futures_util::StreamExt;
use log::info;
use super::ProgressPayload;

#[tauri::command]
pub async fn download_app_update(app: AppHandle, download_url: String) -> Result<(), String> {
    info!("Downloading application update...");

    // 1. Get paths
    let current_exe = env::current_exe().map_err(|e| e.to_string())?;
    let current_dir = current_exe.parent().ok_or("Failed to get exe directory")?;
    let temp_exe = current_dir.join("update_temp.exe");

    // 2. Download the new version
    let client = reqwest::Client::new();
    let res = client.get(&download_url).send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to download update. Server returned: {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(0);
    let mut file = fs::File::create(&temp_exe).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    // 3. Stream and emit progress
    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percent = ((downloaded as f32 / total_size as f32) * 100.0) as u8;
            let _ = app.emit("download-progress", ProgressPayload {
                current_file: "앱 업데이트".to_string(), // Keep this exact string, we check it in the UI!
                percent,
                total_percent: percent,
            });
        }
    }

    // Explicit 100% signal
    let _ = app.emit("download-progress", ProgressPayload {
        current_file: "앱 업데이트 완료".to_string(),
        percent: 100,
        total_percent: 100,
    });

    Ok(())
}

#[tauri::command]
pub fn restart_to_apply_update(app: AppHandle) -> Result<(), String> {
    info!("Applying application update and restarting...");

    let current_exe = env::current_exe().map_err(|e| e.to_string())?;
    let current_dir = current_exe.parent().ok_or("Failed to get exe directory")?;
    let exe_name = current_exe.file_name().unwrap().to_string_lossy();

    let temp_exe = current_dir.join("update_temp.exe");
    let old_exe = current_dir.join(format!("{}.old", exe_name));

    // Clean up old backups
    if old_exe.exists() {
        let _ = fs::remove_file(&old_exe);
    }

    // The Windows Rename Trick
    fs::rename(&current_exe, &old_exe).map_err(|e| format!("Failed to backup current exe: {}", e))?;
    fs::rename(&temp_exe, &current_exe).map_err(|e| format!("Failed to install new exe: {}", e))?;

    // Spawn the new executable
    Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to restart application: {}", e))?;

    // GRACEFUL SHUTDOWN: Let Tauri clean up WebView2 to prevent the Error 1412 crash
    app.exit(0);

    Ok(())
}