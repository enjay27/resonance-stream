use crate::{inject_system_message, SystemLogLevel};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

pub struct DataFactoryJob {
    pub pid: u64,
    pub original: String,
    pub translated: Option<String>,
}

/// The archive lives in the app data folder -- the one "앱 데이터 폴더 열기"
/// opens. (It used to be written three levels up, into the user's home.)
fn dataset_path(app: &AppHandle) -> std::io::Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("dataset_raw.jsonl"))
}

pub fn start_data_factory_worker(app: AppHandle) -> Sender<DataFactoryJob> {
    let (tx, rx): (Sender<DataFactoryJob>, Receiver<DataFactoryJob>) = unbounded();

    thread::spawn(move || {
        let opened = dataset_path(&app).and_then(|path| {
            let file = OpenOptions::new().create(true).append(true).open(&path)?;
            Ok((path, file))
        });
        let (path, file) = match opened {
            Ok(opened) => opened,
            Err(e) => {
                inject_system_message(
                    &app,
                    SystemLogLevel::Error,
                    "DataFactory",
                    format!("Cannot open the chat archive: {}", e),
                );
                return;
            }
        };
        inject_system_message(
            &app,
            SystemLogLevel::Info,
            "DataFactory",
            format!("Archiving chat to {}", path.display()),
        );

        // One open file for the worker's life; flushed whenever the queue
        // runs dry, so a burst is one write and nothing waits long on disk.
        let mut writer = BufWriter::new(file);
        while let Ok(job) = rx.recv() {
            let entry = serde_json::json!({
                "pid": job.pid,
                "original": job.original,
                "translated": job.translated, // Some("text") or null
                "timestamp": now_ms()
            });
            let _ = writeln!(writer, "{}", entry);
            if rx.is_empty() {
                let _ = writer.flush();
            }
        }
        let _ = writer.flush();
    });

    tx
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
