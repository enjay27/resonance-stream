//! The one streaming download used by the model, the llama server and the
//! app updater.

use super::ProgressPayload;
use futures_util::StreamExt;
use resonance_core::download::{check_download_url, DownloadCheck, ProgressThrottle};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// Downloads `url` to `dest`, emitting `download-progress` (labelled
/// `label`) once per whole percent.
///
/// The bytes go to `<dest>.part` first. Only a download that is complete
/// (matches `Content-Length`) and intact (matches `expected_sha256`, when
/// given) is renamed over `dest`, so a failed download never destroys the
/// file that was there before.
pub async fn download_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    label: &str,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    check_download_url(url)?;
    if expected_sha256.is_none() {
        log::warn!(
            "[Download] No SHA-256 published for {}; integrity not verified",
            url
        );
    }

    let res = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!(
            "Download failed. Server returned: {}",
            res.status()
        ));
    }

    let total_size = res.content_length();
    let mut check = DownloadCheck::new(total_size, expected_sha256);
    let mut progress = ProgressThrottle::default();

    let part = part_path(dest);
    let result = async {
        let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut stream = res.bytes_stream();
        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| e.to_string())?;
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            check.update(&chunk);
            if let Some(percent) = progress.update(check.received(), total_size.unwrap_or(0)) {
                let _ = app.emit(
                    "download-progress",
                    ProgressPayload {
                        current_file: label.to_string(),
                        percent,
                        total_percent: percent,
                    },
                );
            }
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        check.finish()?;
        fs::rename(&part, dest).map_err(|e| e.to_string())
    }
    .await;

    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}
