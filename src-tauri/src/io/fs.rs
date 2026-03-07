use std::fs;
use std::io::Write;
use chrono::{Local, TimeZone};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;

// Adjust this import path if ExportMessage is located elsewhere!
use crate::protocol::types::ExportMessage;

#[tauri::command]
pub async fn open_app_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    // 1. Resolve the specific AppData/Roaming folder for this app
    let app_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    // 2. CRITICAL: Ensure the directory exists.
    // If Explorer is called on a non-existent path, it defaults to 'Documents'.
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    }

    // 3. Open the folder using the system file explorer
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(app_dir.to_str().unwrap()) // Pass the absolute AppData path
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(app_dir.to_str().unwrap())
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn export_chat_log(app: tauri::AppHandle, logs: Vec<ExportMessage>) -> Result<String, String> {
    // 1. Get the AppData directory
    let app_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    }

    // 2. Create a unique filename based on the current time
    let timestamp_now = Local::now().format("%Y%m%d_%H%M%S");
    let file_path = app_dir.join(format!("chat_export_{}.txt", timestamp_now));

    // 3. Open the file for writing
    let mut file = fs::File::create(&file_path).map_err(|e| e.to_string())?;

    // 4. Write the header
    writeln!(file, "=== BPSR Translator Chat Export ({}) ===", Local::now().format("%Y-%m-%d %H:%M:%S"))
        .map_err(|e| e.to_string())?;
    writeln!(file, "--------------------------------------------------").map_err(|e| e.to_string())?;

    // 5. Format and write each message
    for log in logs {
        // Convert Unix timestamp to readable date/time
        let dt = Local.timestamp_opt(log.timestamp as i64, 0).unwrap();
        let time_str = dt.format("%Y-%m-%d %H:%M:%S");

        // Format translation (if it exists)
        let trans_str = match &log.translated {
            Some(t) => format!(" -> {}", t),
            None => "".to_string(),
        };

        let line = format!("[{}] [{}] {}: {}{}", time_str, log.channel, log.nickname, log.message, trans_str);
        writeln!(file, "{}", line).map_err(|e| e.to_string())?;
    }

    // Return the path so we could theoretically show it to the user
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_browser(app: tauri::AppHandle, url: String) -> Result<(), String> {
    app.shell().open(url, None).map_err(|e| e.to_string())
}