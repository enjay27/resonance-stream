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
    state: State<AppState>,
    use_gpu: bool
) -> Result<String, String> {
    let model_path = model_manager::get_model_path(&app);
    let model_path_str = model_path.to_string_lossy().to_string();

    if !model_path.exists() {
        return Err("Model file missing".to_string());
    }

    let gpu_arg = if use_gpu { "-1" } else { "0" };

    // 1. Spawn Sidecar
    let sidecar = app.shell().sidecar("translator").map_err(|e| e.to_string())?
        .args(&["--model", &model_path_str])
        .args(&["--gpu_layers", gpu_arg]);

    let (mut rx_sidecar, mut child) = sidecar.spawn().map_err(|e| e.to_string())?;

    // 2. Create Channel
    let (tx, rx_sniffer) = std::sync::mpsc::channel::<String>();

    // 3. Store Sender in State (Cloning ensures channel stays open)
    *state.tx.lock().unwrap() = Some(tx.clone());

    // 4. Start Sniffer
    sniffer::start_sniffer(tx);

    // 5. Start Writer Thread (The "Hand")
    thread::spawn(move || {
        // CRITICAL FIX: Shadow 'child' to ensure it is mutable inside this thread
        let mut child = child;

        println!("[Writer] Thread started. Listening for text...");

        while let Ok(msg) = rx_sniffer.recv() {
            println!("[Writer] Sending to Python: {}", msg); // Debug Log

            let mut msg_with_newline = msg;
            msg_with_newline.push('\n');

            // Try to write. If this fails, Python is likely dead.
            if let Err(e) = child.write(msg_with_newline.as_bytes()) {
                eprintln!("[Writer] ERROR: Failed to write to Python: {}", e);
                break; // Exit thread if pipe is broken
            }
        }

        println!("[Writer] Thread Exiting (Channel Closed or Pipe Broken)");
    });

    // 6. Monitor Output (Python -> UI)
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx_sidecar.recv().await {
            if let CommandEvent::Stdout(line_bytes) = event {
                let line = String::from_utf8_lossy(&line_bytes);
                app.emit("translator-event", line.to_string()).unwrap_or(());
            } else if let CommandEvent::Stderr(line_bytes) = event {
                let line = String::from_utf8_lossy(&line_bytes);
                println!("[PY ERR] {}", line); // Log Python errors to terminal
            }
        }
    });

    Ok("Translator Started".to_string())
}

#[tauri::command]
pub fn manual_translate(text: String, state: State<AppState>) -> Result<(), String> {
    // Debug: Check if we even get here
    println!("[Manual] Request: {}", text);

    let guard = state.tx.lock().unwrap();

    if let Some(tx) = guard.as_ref() {
        let json_msg = serde_json::json!({ "text": text }).to_string();

        // This is the line failing for you
        tx.send(json_msg).map_err(|e| {
            let err_msg = format!("Channel Closed! Writer thread likely died. Error: {}", e);
            eprintln!("{}", err_msg);
            err_msg
        })?;

        println!("[Manual] Sent to channel.");
        Ok(())
    } else {
        println!("[Manual] Error: AI not running.");
        Err("AI not started yet. Click 'Start AI Translator' first.".to_string())
    }
}
