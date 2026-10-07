use super::ProgressPayload;
use log::info;
use parking_lot::Mutex;
use resonance_core::test_env::UpdateState;
use resonance_core::update_feed::UpdateFeed;
use resonance_core::update_signature::verify_with_app_keys;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// The release whose exe is sitting in `update_temp.exe`, once it has passed
/// the signature check. `restart_to_apply_update` checks the file again
/// against it right before the swap.
static DOWNLOADED: Mutex<Option<UpdateFeed>> = Mutex::new(None);

/// Set by `cancel_app_update`; the running download looks at it every second.
static CANCEL: AtomicBool = AtomicBool::new(false);

/// Is a `download_app_update` running? Only one may, so a retry started right
/// after a cancel cannot share `update_temp.exe.part` with the one winding down.
static RUNNING: AtomicBool = AtomicBool::new(false);

struct RunningGuard;

impl RunningGuard {
    fn start() -> Option<Self> {
        (!RUNNING.swap(true, Ordering::SeqCst)).then_some(RunningGuard)
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

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
    let _running = RunningGuard::start().ok_or("An update download is already running")?;
    CANCEL.store(false, Ordering::SeqCst);

    crate::test_env::report_update(UpdateState::Downloading);
    let result = download_announced(&app, download_url).await;
    crate::test_env::report_update(match &result {
        Ok(()) => UpdateState::Downloaded,
        Err(e) => UpdateState::Error(e.clone()),
    });
    result
}

/// The download itself: the announced release into `update_temp.exe`, checked.
async fn download_announced(app: &AppHandle, download_url: String) -> Result<(), String> {
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
    super::fetch::download_file_cancellable(
        app,
        &feed.url,
        &temp_exe,
        "앱 업데이트", // Keep this exact string, we check it in the UI!
        None,
        Some(&CANCEL),
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

/// Stops the running update download and returns once it has let go of its
/// files (a few seconds at most), so the caller may start another at once.
/// A download that was cancelled is rejected with `UPDATE_CANCELLED`.
#[tauri::command]
pub async fn cancel_app_update() {
    CANCEL.store(true, Ordering::SeqCst);
    for _ in 0..100 {
        if !RUNNING.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
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

    // The Windows Rename Trick; puts the running exe back if the new one cannot be moved in.
    resonance_core::download::install_swap(&current_exe, &temp_exe, &old_exe)?;

    // Spawn the new executable
    // A test run keeps its flags; `--fresh` and `--print-env` never carry over.
    Command::new(&current_exe)
        .args(crate::test_env::restart_args())
        .env_remove("RESONANCE_TEST_FRESH")
        .env_remove("RESONANCE_TEST_PRINT_ENV")
        .spawn()
        .map_err(|e| format!("Failed to restart application: {}", e))?;

    // GRACEFUL SHUTDOWN: Let Tauri clean up WebView2 to prevent the Error 1412 crash
    app.exit(0);

    Ok(())
}
