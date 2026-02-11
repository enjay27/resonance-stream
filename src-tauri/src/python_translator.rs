use tauri::{AppHandle, Manager, Emitter, State}; // Added State
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;
use std::sync::{Arc, Mutex}; // Added Mutex
use std::thread;
use crate::model_manager;
use crate::sniffer;

// 1. Define State to hold the Channel
pub struct AppState {
    pub tx: Mutex<Option<tauri_plugin_shell::process::CommandChild>>,
}

#[tauri::command]
pub fn start_translator_sidecar(
    app: AppHandle,
    state: State<'_, AppState>,
    use_gpu: bool
) -> Result<String, String> {
    println!("Start Translator Sidecar");

    let model_path = model_manager::get_model_path(&app);
    let device_arg = if use_gpu { "cuda" } else { "cpu" };

    // 1. Spawn the sidecar
    // Note: "python_translator" must match exactly in tauri.conf.json
    let (mut rx, child) = app
        .shell()
        .sidecar("python_translator")
        .map_err(|e| e.to_string())?
        .args(["--model", &model_path, "--device", device_arg])
        .spawn()
        .map_err(|e| format!("Spawn failed: {}", e))?;

    // 2. STORE THE CHILD IN STATE
    {
        let mut tx_guard = state.tx.lock().unwrap();
        *tx_guard = Some(child); // This is why you were getting 'None'
    }

    // 3. Handle Stdout (Async)
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let CommandEvent::Stdout(line_bytes) = event {
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                let _ = app_clone.emit("translator-event", line);
            }
        }
    });

    Ok("Sidecar started and stored in state".into())
}

#[tauri::command]
pub async fn manual_translate(
    text: String,
    id: u64,
    state: State<'_, AppState>
) -> Result<(), String> {
    let mut guard = state.tx.lock().unwrap();

    // Check if the sidecar is actually there
    if let Some(child) = guard.as_mut() {
        let msg = serde_json::json!({ "text": text, "id": id }).to_string() + "\n";

        // Write to Python's stdin
        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;
        println!("[Rust] Sent to Python: {}", text);
        Ok(())
    } else {
        println!("[Error] Python process is None. Did you call start_translator_sidecar?");
        Err("Translator sidecar is not running".into())
    }
}