use super::ProgressPayload;
use log::info;
use parking_lot::Mutex;
use resonance_core::update_feed::UpdateFeed;
use resonance_core::update_signature::verify_with_app_keys;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use tauri::{AppHandle, Emitter};

/// The release whose exe is sitting in `update_temp.exe`, once it has passed
/// the signature check. `restart_to_apply_update` checks the file again
/// against it right before the swap.
static DOWNLOADED: Mutex<Option<UpdateFeed>> = Mutex::new(None);

/// Is the exe at `exe` signed by one of the app's built-in keys, for the
/// version `feed` announced? Reads the whole file: it is checked as it will
/// be installed.
fn verify_file(exe: &Path, feed: &UpdateFeed) -> Result<(), String> {
    let data = fs::read(exe).map_err(|e| format!("Could not read the update to check it: {e}"))?;
    verify_with_app_keys(&data, &feed.signature, &feed.version).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_app_update(app: AppHandle, download_url: String) -> Result<(), String> {
    info!("Downloading application update...");

    // 1. Only the release the last check announced.
    let feed = super::gist::announced_update()
        .ok_or("No update has been announced; check for updates first")?;
    if download_url != feed.url {
        return Err("The update to download is not the announced one".to_string());
    }
    *DOWNLOADED.lock() = None;

    // 2. Get paths
    let current_exe = env::current_exe().map_err(|e| e.to_string())?;
    let current_dir = current_exe.parent().ok_or("Failed to get exe directory")?;
    let temp_exe = current_dir.join("update_temp.exe");

    // 3. Download the new version (HTTPS only). The exe runs as
    // Administrator after restart, so it is only installed when one of the
    // app's built-in keys signed it for exactly the announced version.
    super::fetch::download_file(
        &app,
        &feed.url,
        &temp_exe,
        "앱 업데이트", // Keep this exact string, we check it in the UI!
        None,
    )
    .await?;

    // 4. Check the signature; a file that fails is thrown away.
    if let Err(e) = verify_file(&temp_exe, &feed) {
        let _ = fs::remove_file(&temp_exe);
        return Err(e);
    }
    *DOWNLOADED.lock() = Some(feed);

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

    // Check again what is about to be installed: the file has sat on disk
    // since the download.
    let feed = DOWNLOADED
        .lock()
        .clone()
        .ok_or("No verified update has been downloaded")?;
    verify_file(&temp_exe, &feed)?;

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
