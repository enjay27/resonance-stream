use super::ProgressPayload;
use log::info;
use resonance_core::download::check_download_url;
use resonance_core::update_signature::{signature_url, verify_with_app_keys};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use tauri::{AppHandle, Emitter};

/// A minisign signature is a few hundred bytes; anything bigger is not one.
const SIGNATURE_MAX_BYTES: usize = 4096;

/// The signature published next to the update (`<download_url>.sig`).
async fn fetch_signature(url: &str) -> Result<String, String> {
    check_download_url(url)?;
    let mut res = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!(
            "Could not fetch the update signature. Server returned: {}",
            res.status()
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        body.extend_from_slice(&chunk);
        if body.len() > SIGNATURE_MAX_BYTES {
            return Err("The update signature is too large".to_string());
        }
    }
    String::from_utf8(body).map_err(|_| "The update signature is not text".to_string())
}

/// Is the exe at `exe` signed by one of the app's built-in keys, for
/// `version`? Reads the whole file: it is checked as it will be installed.
fn verify_file(exe: &Path, signature: &str, version: &str) -> Result<(), String> {
    let data = fs::read(exe).map_err(|e| format!("Could not read the update to check it: {e}"))?;
    verify_with_app_keys(&data, signature, version).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_app_update(app: AppHandle, download_url: String) -> Result<(), String> {
    info!("Downloading application update...");

    // 1. Get paths
    let current_exe = env::current_exe().map_err(|e| e.to_string())?;
    let current_dir = current_exe.parent().ok_or("Failed to get exe directory")?;
    let temp_exe = current_dir.join("update_temp.exe");

    let sig_path = temp_exe.with_extension("exe.sig");
    let version = super::gist::published_app_version()
        .ok_or("The update check has not announced a version; refusing to install")?;

    // 2. Download the new version (HTTPS only). The exe runs as
    // Administrator after restart, so it is only installed when one of the
    // app's built-in keys signed it for exactly the announced version; a
    // published SHA-256 is still checked on top.
    let sha256 = super::gist::published_sha256(&download_url);
    super::fetch::download_file(
        &app,
        &download_url,
        &temp_exe,
        "앱 업데이트", // Keep this exact string, we check it in the UI!
        sha256.as_deref(),
    )
    .await?;

    // 3. Check the signature; a file that fails is thrown away.
    let checked = async {
        let signature = fetch_signature(&signature_url(&download_url)).await?;
        verify_file(&temp_exe, &signature, &version)?;
        fs::write(&sig_path, signature).map_err(|e| e.to_string())
    }
    .await;
    if let Err(e) = checked {
        let _ = fs::remove_file(&temp_exe);
        let _ = fs::remove_file(&sig_path);
        return Err(e);
    }

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
    let sig_path = temp_exe.with_extension("exe.sig");
    let old_exe = current_dir.join(format!("{}.old", exe_name));

    // Check again what is about to be installed: the file has sat on disk
    // since the download.
    let version = super::gist::published_app_version()
        .ok_or("The update check has not announced a version; refusing to install")?;
    let signature = fs::read_to_string(&sig_path)
        .map_err(|_| "The update has no signature file; download it again".to_string())?;
    verify_file(&temp_exe, &signature, &version)?;

    // Clean up old backups
    if old_exe.exists() {
        let _ = fs::remove_file(&old_exe);
    }

    // The Windows Rename Trick
    fs::rename(&current_exe, &old_exe)
        .map_err(|e| format!("Failed to backup current exe: {}", e))?;
    fs::rename(&temp_exe, &current_exe).map_err(|e| format!("Failed to install new exe: {}", e))?;
    let _ = fs::remove_file(&sig_path);

    // Spawn the new executable
    Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to restart application: {}", e))?;

    // GRACEFUL SHUTDOWN: Let Tauri clean up WebView2 to prevent the Error 1412 crash
    app.exit(0);

    Ok(())
}
