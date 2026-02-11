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
    state: State<AppState>,
    use_gpu: bool
) -> Result<String, String> {
    // 1. Resolve the path to the NLLB INT8 model folder
    let model_path = model_manager::get_model_path(&app);

    // 2. Determine hardware device
    let device_arg = if use_gpu { "cuda" } else { "cpu" };

    // 3. Create the Sidecar Command
    // "python_translator" must match the name in tauri.conf.json -> bundle -> externalBin
    let sidecar_command = app
        .shell()
        .sidecar("python_translator")
        .map_err(|e| format!("Sidecar configuration error: {}", e))?
        .args(["--model", &model_path, "--device", device_arg]);

    // 4. Spawn the process
    let (mut rx, mut child) = sidecar_command
        .spawn()
        .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

    // 5. Shared State for IPC
    // Store the child handle in AppState so we can write to its stdin later
    let state_inner = state.inner().clone();
    {
        let mut tx_guard = state_inner.tx.lock().unwrap();
        // Since we use the child directly to write to stdin
        *tx_guard = Some(child);
    }

    // 6. Handle Sidecar Output (Stdout/Stderr)
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes).to_string();
                    // Emit raw status or results to the Frontend listener
                    println!("[Python] {}", line);
                    let _ = app_clone.emit("translator-event", line);
                }
                CommandEvent::Stderr(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes).to_string();
                    eprintln!("[Python Sidecar Error] {}", line);
                }
                _ => {}
            }
        }
    });

    Ok("Translator sidecar initialized successfully".into())
}

#[tauri::command]
pub async fn manual_translate(
    text: String,
    id: u64,
    state: State<'_, AppState>
) -> Result<(), String> {
    let mut guard = state.tx.lock().unwrap();
    println!("[Manual Translate] {:?}", text);

    if let Some(child) = guard.as_mut() {
        // Construct JSON to send to Python's stdin
        let msg = serde_json::json!({
            "text": text,
            "id": id
        }).to_string() + "\n";

        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Translator sidecar is not running".into())
    }
}