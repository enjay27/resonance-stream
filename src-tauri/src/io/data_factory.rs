use crossbeam_channel::{unbounded, Receiver, Sender};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::{fs, thread};
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::Local;
use tauri::{AppHandle, Manager, State};
use crate::{AppState, ChatMessage};

pub struct DataFactoryJob {
    pub chat: crate::protocol::types::ChatMessage, // Now takes the whole object!
}

pub fn start_data_factory_worker(app: AppHandle) -> Sender<DataFactoryJob> {
    let (tx, rx): (Sender<DataFactoryJob>, Receiver<DataFactoryJob>) = unbounded();

    thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            let _ = append_chat_to_daily_file(&app, &job.chat);
        }
    });

    tx
}

pub fn save_to_data_factory(
    app: &AppHandle,
    pid: u64,
    original: &str,
    translated: &str,
) -> std::io::Result<()> {
    // 1. Get the AppData directory for your app
    let mut path = app
        .path()
        .app_data_dir()
        .expect("Failed to get AppData dir");

    // 2. Ensure the directory exists
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }

    path.push("../../../dataset_raw.jsonl");

    // 3. Prepare the JSON Line
    let entry = serde_json::json!({
        "pid": pid,
        "original": original,
        "translated": translated,
        "timestamp": now_ms()
    });

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    // 4. Write with a newline
    writeln!(file, "{}", entry.to_string())?;

    Ok(())
}

fn append_to_file(
    app: &AppHandle,
    pid: u64,
    original: &str,
    translated: Option<&str>,
) -> std::io::Result<()> {
    let mut path = app
        .path()
        .app_data_dir()
        .expect("Failed to get AppData dir");
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }
    path.push("../../../dataset_raw.jsonl");

    let entry = serde_json::json!({
        "pid": pid,
        "original": original,
        "translated": translated, // Serde automatically handles Some("text") or None (null)
        "timestamp": now_ms()
    });

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    writeln!(file, "{}", entry.to_string())?;
    Ok(())
}

fn append_chat_to_daily_file(
    app: &AppHandle,
    chat: &crate::protocol::types::ChatMessage,
) -> std::io::Result<()> {
    let mut path = app
        .path()
        .app_data_dir()
        .expect("Failed to get AppData dir");

    // Save to a new dedicated folder
    path.push("chat_logs");
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }

    // Automatically names the file based on the day (e.g. 2026-03-12.jsonl)
    let date_str = Local::now().format("%Y-%m-%d").to_string();
    path.push(format!("{}.jsonl", date_str));

    // Append as JSONL
    if let Ok(json_str) = serde_json::to_string(chat) {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{}", json_str)?;
    }

    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tauri::command]
pub fn get_chat_history(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ChatMessage>, String> {
    // 1. Get the limit from user settings
    let config = crate::config::load_config(app.clone());
    let limit = config.chat_limit;

    // 2. Locate the chat_logs directory
    let mut path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    path.push("chat_logs");

    if !path.exists() {
        return Ok(Vec::new());
    }

    // 3. Find all .jsonl files and sort them (Alphabetical sorting automatically sorts YYYY-MM-DD by date)
    let mut files: Vec<_> = fs::read_dir(&path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "jsonl"))
        .map(|entry| entry.path())
        .collect();

    files.sort();

    let mut messages = Vec::new();
    let mut max_pid: u64 = 0;

    // 4. Read the files backwards (newest day first)
    for file_path in files.into_iter().rev() {
        if let Ok(file) = fs::File::open(&file_path) {
            let reader = BufReader::new(file);
            let lines: Vec<_> = reader.lines().filter_map(|l| l.ok()).collect();

            // Read lines backwards (newest message first)
            for line in lines.into_iter().rev() {
                if let Ok(chat) = serde_json::from_str::<ChatMessage>(&line) {

                    // Track the highest PID so new messages don't collide!
                    if chat.pid > max_pid {
                        max_pid = chat.pid;
                    }

                    messages.push(chat.clone());

                    if messages.len() >= limit {
                        break;
                    }
                }
            }
        }
        if messages.len() >= limit {
            break;
        }
    }

    // 5. Reverse the list so it flows chronologically (oldest at top, newest at bottom)
    messages.reverse();

    println!("messages {:?}", messages);

    // 6. CRITICAL: Update the backend's PID counter so the next captured packet gets a unique ID
    state.next_pid.store(max_pid + 1, std::sync::atomic::Ordering::SeqCst);

    // 7. Hydrate the backend's in-memory history (Needed so retroactive blocking still works!)
    let mut history_lock = state.chat_history.lock().unwrap();
    for msg in &messages {
        history_lock.insert(msg.pid, msg.clone());
    }

    Ok(messages)
}