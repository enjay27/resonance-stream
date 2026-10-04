//! Debug: append the game's raw port-5003 packets to a file (W2), toggled live
//! from the settings. The format and the size cap live in
//! `resonance_core::capture`; this is only the file and the switch.

use crate::inject_system_message;
use crate::protocol::types::SystemLogLevel;
use resonance_core::capture::{RawCaptureWriter, Recorded, DEFAULT_MAX_BYTES};
use std::fs::{self, File};
use std::io::BufWriter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

const FLUSH_EVERY: Duration = Duration::from_secs(1);

static RAW_CAPTURE: AtomicBool = AtomicBool::new(false);

/// The switch the settings flip; the sniffer loop notices on its next packet.
pub fn set_raw_capture(on: bool) {
    RAW_CAPTURE.store(on, Ordering::Relaxed);
}

/// `<app data>/captures`, created if missing.
fn captures_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = crate::app_dirs::data(app)
        .map_err(|e| e.to_string())?
        .join("captures");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[tauri::command]
pub async fn open_captures_folder(app: AppHandle) -> Result<(), String> {
    let dir = captures_dir(&app)?;
    std::process::Command::new("explorer")
        .arg(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Owned by the sniffer loop: opens a file when the switch goes on, closes it
/// when it goes off or the size cap is hit.
#[derive(Default)]
pub struct RawCapture {
    writer: Option<RawCaptureWriter<BufWriter<File>>>,
    /// The cap was hit; stays shut until the switch is turned off and on again.
    full: bool,
    last_flush: Option<Instant>,
}

impl RawCapture {
    pub fn feed(&mut self, app: &AppHandle, packet: &[u8]) {
        if !RAW_CAPTURE.load(Ordering::Relaxed) {
            if self.writer.take().is_some() {
                inject_system_message(app, SystemLogLevel::Info, "Capture", "Raw capture stopped.");
            }
            self.full = false;
            return;
        }
        if self.full {
            return;
        }
        if self.writer.is_none() {
            match Self::open(app) {
                Ok(writer) => self.writer = Some(writer),
                Err(e) => {
                    RAW_CAPTURE.store(false, Ordering::Relaxed);
                    inject_system_message(
                        app,
                        SystemLogLevel::Error,
                        "Capture",
                        format!("Raw capture not started: {e}"),
                    );
                    return;
                }
            }
        }
        let Some(writer) = self.writer.as_mut() else {
            return;
        };

        let ts_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        match writer.record(ts_ms, packet) {
            Ok(Recorded::Written) => {
                if self.last_flush.map_or(true, |t| t.elapsed() >= FLUSH_EVERY) {
                    let _ = writer.flush();
                    self.last_flush = Some(Instant::now());
                }
            }
            Ok(Recorded::Skipped) => {}
            Ok(Recorded::Full) => {
                let _ = writer.flush();
                self.writer = None;
                self.full = true;
                inject_system_message(
                    app,
                    SystemLogLevel::Warning,
                    "Capture",
                    "Raw capture file reached its size cap; capture stopped. Toggle it off and on for a new file.",
                );
            }
            Err(e) => {
                self.writer = None;
                self.full = true;
                inject_system_message(
                    app,
                    SystemLogLevel::Error,
                    "Capture",
                    format!("Raw capture write failed: {e}"),
                );
            }
        }
    }

    fn open(app: &AppHandle) -> Result<RawCaptureWriter<BufWriter<File>>, String> {
        let name = format!(
            "capture-{}.log",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        );
        let path = captures_dir(app)?.join(name);
        let file = File::create(&path).map_err(|e| e.to_string())?;
        inject_system_message(
            app,
            SystemLogLevel::Info,
            "Capture",
            format!("Raw capture started: {}", path.display()),
        );
        Ok(RawCaptureWriter::new(
            BufWriter::new(file),
            DEFAULT_MAX_BYTES,
        ))
    }
}
