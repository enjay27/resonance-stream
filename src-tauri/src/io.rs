use std::io::Write;
use std::fs::OpenOptions;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::thread;

pub struct DataFactoryJob {
    pub pid: u64,
    pub original: String,
    pub translated: Option<String>,
}

pub fn start_data_factory_worker(app: AppHandle) -> Sender<DataFactoryJob> {
    let (tx, rx): (Sender<DataFactoryJob>, Receiver<DataFactoryJob>) = unbounded();

    thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            // as_deref() converts Option<String> to Option<&str>
            let _ = append_to_file(&app, job.pid, &job.original, job.translated.as_deref());
        }
    });

    tx
}

pub fn save_to_data_factory(app: &AppHandle, pid: u64, original: &str, translated: &str) -> std::io::Result<()> {
    // 1. Get the AppData directory for your app
    let mut path = app.path().app_data_dir().expect("Failed to get AppData dir");

    // 2. Ensure the directory exists
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }

    path.push("dataset_raw.jsonl");

    // 3. Prepare the JSON Line
    let entry = serde_json::json!({
        "pid": pid,
        "original": original,
        "translated": translated,
        "timestamp": now_ms()
    });

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    // 4. Write with a newline
    writeln!(file, "{}", entry.to_string())?;

    Ok(())
}

fn append_to_file(app: &AppHandle, pid: u64, original: &str, translated: Option<&str>) -> std::io::Result<()> {
    let mut path = app.path().app_data_dir().expect("Failed to get AppData dir");
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }
    path.push("dataset_raw.jsonl");

    let entry = serde_json::json!({
        "pid": pid,
        "original": original,
        "translated": translated, // Serde automatically handles Some("text") or None (null)
        "timestamp": now_ms()
    });

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file, "{}", entry.to_string())?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}