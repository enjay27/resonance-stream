use std::io::Write;
use std::fs::OpenOptions;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

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

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}