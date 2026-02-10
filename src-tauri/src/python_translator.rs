use tauri::{AppHandle, Manager, Emitter, State}; // Added State
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;
use std::sync::{Arc, Mutex}; // Added Mutex
use std::thread;
use crate::model_manager;
use crate::sniffer;

// 1. Define State to hold the Channel
pub struct AppState {
    pub(crate) tx: Mutex<Option<std::sync::mpsc::Sender<String>>>,
}

#[tauri::command]
pub fn start_translator_sidecar(
    app: AppHandle,
    state: State<AppState>, // Access State
    use_gpu: bool
) -> Result<String, String> {
    let model_path = model_manager::get_model_path(&app);
    let model_path_str = model_path.to_string_lossy().to_string();

    if !model_path.exists() {
        return Err("Model file missing".to_string());
    }

    let gpu_arg = if use_gpu { "-1" } else { "0" };

    let sidecar = app.shell().sidecar("translator").map_err(|e| e.to_string())?
        .args(&["--model", &model_path_str])
        .args(&["--gpu_layers", gpu_arg]);

    let (mut rx_sidecar, mut child) = sidecar.spawn().map_err(|e| e.to_string())?;

    // 2. Create Channel
    let (tx, rx_sniffer) = std::sync::mpsc::channel::<String>();

    // 3. STORE THE SENDER IN STATE (So we can use it manually)
    *state.tx.lock().unwrap() = Some(tx.clone());

    // 4. Start Sniffer
    sniffer::start_sniffer(tx); // Sniffer gets a copy

    // 5. Start Writer Loop (Reads from Channel -> Writes to Python)
    std::thread::spawn(move || {
        while let Ok(msg) = rx_sniffer.recv() {
            let mut msg_with_newline = msg;
            msg_with_newline.push('\n');
            let _ = child.write(msg_with_newline.as_bytes());
        }
    });

    // Monitor Output (Python -> UI)
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx_sidecar.recv().await {
            if let CommandEvent::Stdout(line_bytes) = event {
                let line = String::from_utf8_lossy(&line_bytes);
                app.emit("translator-event", line.to_string()).unwrap_or(());
            }
        }
    });

    Ok("Translator Started".to_string())
}

#[tauri::command]
pub fn manual_translate(text: String, state: State<AppState>) -> Result<(), String> {
    let guard = state.tx.lock().unwrap();
    if let Some(tx) = guard.as_ref() {
        // Format as JSON {"text": "..."}
        let json_msg = serde_json::json!({ "text": text }).to_string();
        tx.send(json_msg).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("AI not started yet".to_string())
    }
}
