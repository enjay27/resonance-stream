use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use std::sync::Mutex;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use crate::{inject_system_message, model_manager};
use crate::sniffer::{AppState, ChatPacket};

// 1. Define State to hold the Channel


#[tauri::command]
pub async fn start_translator_sidecar(
    window: Window,
    app: AppHandle,
    state: State<'_, AppState>,
    use_gpu: bool
) -> Result<String, String> {
    inject_system_message(&window, format!("[Sidecar] Request received to start translator. GPU: {}", use_gpu));

    let model_path = model_manager::get_model_path(&app);
    let device_arg = if use_gpu { "cuda" } else { "cpu" };

    // 1. Spawn the sidecar
    // Make sure your tauri.conf.json -> externalBin contains "binaries/python_translator"
    let (mut rx, child) = app
        .shell()
        .sidecar("translator")
        .map_err(|e| {
            let err = format!("[Sidecar] ERROR: Binary not found or config mismatch: {}", e);
            inject_system_message(&window, err.clone());
            err
        })?
        .args(["--model", &model_path, "--device", device_arg])
        .spawn()
        .map_err(|e| {
            let err = format!("[Sidecar] ERROR: Failed to execute binary: {}", e);
            inject_system_message(&window, err.clone());
            err
        })?;

    inject_system_message(&window, format!("[Sidecar] Process spawned successfully. Child PID: {}", child.pid()));

    // 2. STORE THE CHILD IN STATE
    {
        let mut tx_guard = state.tx.lock().unwrap();
        *tx_guard = Some(child);
        inject_system_message(&window, "[Sidecar] SUCCESS: Child handle saved to AppState.");
    }

    // 3. Handle Stdout (Status & Results)
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let CommandEvent::Stdout(line_bytes) = event {
                let line = String::from_utf8_lossy(&line_bytes).to_string();

                println!("[Python] {}", line);

                // Try to parse the JSON from Python
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    if json["type"] == "status" {
                        // It's a status message! Inject it into the system log.
                        let status_msg = json["message"].as_str().unwrap_or("Unknown Status");

                        // We call the logger we built in the previous step
                        inject_system_message(&window, format!("[Python] {}", status_msg));

                        // Also update the badge status if it's "Ready"
                        if status_msg.contains("Ready") {
                            let _ = app_clone.emit("translator-status", "Connected");
                        }
                    } else if json["type"] == "result" {
                        let target_pid = json["pid"].as_u64().unwrap_or(0);
                        let translated_text = json["translated"].as_str().unwrap_or_default().to_string();

                        if let Some(state) = app_clone.try_state::<crate::AppState>() {
                            let mut history = state.chat_history.lock().unwrap();

                            if let Some(packet) = history.get_mut(&target_pid) {
                                packet.translated = Some(translated_text);
                            }
                        }

                        // Send it to the translator-event listener in app.rs
                        let _ = app_clone.emit("translator-event", line);
                    }
                }
            }
        }
        inject_system_message(&window, "[Sidecar] WARNING: Stdout stream closed.");
        let _ = app_clone.emit("translator-status", "Disconnected");
    });

    Ok("Connected".into())
}

#[tauri::command]
pub async fn manual_translate(
    text: String,
    pid: u64,
    state: State<'_, AppState>
) -> Result<(), String> {
    // 1. Diagnostics
    println!("[Diagnostic] manual_translate called for: {}", text);

    let mut guard = state.tx.lock().unwrap();

    // 2. Check the child
    if let Some(child) = guard.as_mut() {
        let msg = serde_json::json!({ "text": text, "pid": pid }).to_string() + "\n";

        child.write(msg.as_bytes()).map_err(|e| {
            println!("[Error] Pipe write failed: {}", e);
            e.to_string()
        })?;

        println!("[Rust] Message piped to Python stdin.");
        Ok(())
    } else {
        // If we reach here, start_translator_sidecar was NEVER successful
        println!("[Error] Python process is None in state.");
        Err("Translator not initialized".into())
    }
}