use super::ProgressPayload;
use log::info;
use std::env;
use std::fs;
use std::process::Command;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn download_app_update(app: AppHandle, download_url: String) -> Result<(), String> {
    info!("Downloading application update...");

    // 1. Get paths
    let current_exe = env::current_exe().map_err(|e| e.to_string())?;
    let current_dir = current_exe.parent().ok_or("Failed to get exe directory")?;
    let temp_exe = current_dir.join("update_temp.exe");

    // 2. Download the new version (HTTPS only; verified when the gist
    // publishes a SHA-256). The exe runs as Administrator after restart.
    super::fetch::download_file(
        &app,
        &download_url,
        &temp_exe,
        "앱 업데이트", // Keep this exact string, we check it in the UI!
        super::gist::published_sha256(&download_url).as_deref(),
    )
    .await?;

    // Explicit 100% signal
    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            current_file: "앱 업데이트 완료".to_string(),
            percent: 100,
            total_percent: 100,
        },
    );

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
    fs::rename(&current_exe, &old_exe)
        .map_err(|e| format!("Failed to backup current exe: {}", e))?;
    fs::rename(&temp_exe, &current_exe).map_err(|e| format!("Failed to install new exe: {}", e))?;

    // Spawn the new executable
    Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to restart application: {}", e))?;

    // GRACEFUL SHUTDOWN: Let Tauri clean up WebView2 to prevent the Error 1412 crash
    app.exit(0);

    Ok(())
}
