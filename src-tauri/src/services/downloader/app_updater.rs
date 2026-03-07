use std::env;
use std::fs;
use std::io::Write;
use std::process::Command;
use tauri::{AppHandle, Emitter};
use futures_util::StreamExt;
use super::ProgressPayload;

#[tauri::command]
pub async fn update_application(app: AppHandle, download_url: String) -> Result<(), String> {
    // 1. Get the path of the currently running .exe
    let current_exe = env::current_exe().map_err(|e| e.to_string())?;
    let current_dir = current_exe.parent().ok_or("Failed to get exe directory")?;
    let exe_name = current_exe.file_name().unwrap().to_string_lossy();

    // 2. Define our temp and old file paths
    let temp_exe = current_dir.join("update_temp.exe");
    let old_exe = current_dir.join(format!("{}.old", exe_name));

    // Clean up any .old files from previous updates
    if old_exe.exists() {
        let _ = fs::remove_file(&old_exe);
    }

    // 3. Download the new version
    let client = reqwest::Client::new();
    let res = client.get(&download_url).send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to download update. Server returned: {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(0);
    let mut file = fs::File::create(&temp_exe).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percent = ((downloaded as f32 / total_size as f32) * 100.0) as u8;
            let _ = app.emit("download-progress", ProgressPayload {
                current_file: "앱 업데이트 다운로드 중...".to_string(),
                percent,
                total_percent: percent,
            });
        }
    }

    let _ = app.emit("download-progress", ProgressPayload {
        current_file: "업데이트 적용 및 재시작 중...".to_string(),
        percent: 100,
        total_percent: 100,
    });

    // 4. The Windows Rename Trick
    // Rename current running app to .old (Windows allows renaming open files!)
    fs::rename(&current_exe, &old_exe).map_err(|e| format!("Failed to backup current exe: {}", e))?;

    // Move the downloaded temp file into the original app's place
    fs::rename(&temp_exe, &current_exe).map_err(|e| format!("Failed to install new exe: {}", e))?;

    // 5. Spawn the new executable and exit the old one
    Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to restart application: {}", e))?;

    std::process::exit(0);
}