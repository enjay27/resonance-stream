use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use crate::{inject_system_message, model_manager};
use crate::sniffer::{AppState, ChatPacket};

// 1. Define State to hold the Channel

// --- REQUEST: Rust -> Python ---
#[derive(Serialize)]
pub struct TranslationRequest {
    pub text: String,
    pub pid: u64,
    pub nickname: Option<String>,
}

// --- RESPONSE: Python -> Rust ---
#[derive(Deserialize, Debug, Clone)]
pub struct TranslationResponse {
    pub pid: u64,
    pub translated: String,
    #[serde(rename = "nickname_info")]
    pub nickname_info: Option<NicknameInfo>,
    pub diagnostics: Option<serde_json::Value>, // For --debug mode
}

#[derive(Deserialize, Debug, Clone)]
pub struct NicknameInfo {
    pub original: String,
    pub romanized: String,
    pub display: String,
}


#[tauri::command]
pub async fn start_translator_sidecar(
    window: Window,
    app: AppHandle,
    state: State<'_, AppState>,
    use_gpu: bool
) -> Result<String, String> {
    inject_system_message(&app, format!("[Sidecar] Request received. GPU: {}", use_gpu));

    let model_path = model_manager::get_model_path(&app);

    // Resolve the local dictionary path in AppData
    let dict_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("custom_dict.json");

    // 1. Spawn the sidecar (Note: mut child is needed for some operations)
    let (mut rx, child) = app
        .shell()
        .sidecar("translator")
        .map_err(|e| format!("[Sidecar] Binary not found: {}", e))?
        .args(["--model", &model_path, "--dict", dict_path.to_str().unwrap()])
        .spawn()
        .map_err(|e| format!("[Sidecar] Failed to execute: {}", e))?;

    // 2. STORE THE CHILD IN STATE (Transfer Ownership)
    {
        let mut tx_guard = state.tx.lock().unwrap();
        // Move 'child' into the Mutex. DO NOT CLONE.
        *tx_guard = Some(child);
        inject_system_message(&app, "[Sidecar] SUCCESS: Child handle saved to AppState.");
    }

    // 3. Handle Stdout (Unchanged logic, but uses updated State for O(1) history)
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let CommandEvent::Stdout(line_bytes) = event {
                let line = String::from_utf8_lossy(&line_bytes).to_string();

                // Use the new struct to parse the result
                if let Ok(resp) = serde_json::from_str::<TranslationResponse>(&line) {
                    let target_pid = resp.pid;
                    let translated_text = resp.translated;
                    println!("[Python] Translated: {:?}", translated_text);
                    println!("[Python] Diagnostic: {:?}", resp.diagnostics);

                    if let Some(state) = app_clone.try_state::<crate::AppState>() {
                        let mut history = state.chat_history.lock().unwrap();
                        if let Some(packet) = history.get_mut(&target_pid) {
                            packet.translated = Some(translated_text);

                            // You can now access the romanized nickname here
                            if let Some(info) = resp.nickname_info {
                                println!("[Python] Nickname: {:?}", info);
                            }
                        }
                    }
                    // Emit to Frontend
                    let _ = app_clone.emit("translator-event", &line);
                }
            }
        }
        let _ = app_clone.emit("translator-status", "Disconnected");
    });
    Ok("Connected".into())
}



#[tauri::command]
pub async fn manual_translate(
    text: String,
    pid: u64,
    nickname: Option<String>, // Added nickname parameter
    state: State<'_, AppState>
) -> Result<(), String> {
    // 1. Diagnostics
    println!("[Diagnostic] manual_translate called for: {}", text);

    let mut guard = state.tx.lock().unwrap();

    if let Some(child) = guard.as_mut() {
        // Use the struct for type safety
        let request = TranslationRequest { text, pid, nickname };
        let msg = serde_json::to_string(&request).map_err(|e| e.to_string())? + "\n";

        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;

        println!("[Rust] Message piped to Python stdin.");
        Ok(())
    } else {
        // If we reach here, start_translator_sidecar was NEVER successful
        println!("[Error] Python process is None in state.");
        Err("Translator not initialized".into())
    }
}