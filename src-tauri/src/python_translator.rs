use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use crate::{inject_system_message, model_manager};
use crate::sniffer::{AppState, ChatPacket, SystemLogLevel};

// 1. Define State to hold the Channel

// --- REQUEST: Rust -> Python ---
#[derive(Serialize)]
pub struct NicknameRequest {
    pub cmd: String,          // Always "nickname_only"
    pub pid: u64,
    pub nickname: String,     // Required for this request type
}

#[derive(Serialize)]
pub struct MessageRequest {
    pub cmd: String,          // Always "translate"
    pub pid: u64,
    pub text: String,         // The Japanese message
}

// --- RESPONSE: Python -> Rust ---
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NicknameResponse {
    pub pid: u64,
    pub nickname: String,
    pub romaji: String, // Flat string, no object
}

// --- For Full translation requests ---
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageResponse {
    pub pid: u64,
    pub translated: String,
}


#[tauri::command]
pub async fn start_translator_sidecar(
    window: Window,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = crate::config::load_config(app.clone());

    if !config.use_translation {
        inject_system_message(&app, SystemLogLevel::Info, "Sidecar", "Translation is disabled in settings. Skipping startup.");
        return Ok("Disabled".into());
    }

    inject_system_message(&app, SystemLogLevel::Info, "Sidecar", format!("Starting Sidecar with Device: {}, Tier: {}", config.compute_mode, config.tier));
    let version = app.package_info().version.to_string();
    let model_path = model_manager::get_model_path(&app);

    // Resolve the local dictionary path in AppData
    let dict_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("custom_dict.json");

    // 1. Spawn the sidecar (Note: mut child is needed for some operations)
    let mut args = vec![
        "--model", &model_path,
        "--dict", dict_path.to_str().unwrap(),
        "--tier", &config.tier, // Use the tier from config
        "--device", &config.compute_mode,
        "--version", &version,
    ];
    if config.is_debug {
        args.push("--debug");
    }
    let (mut rx, child) = app
        .shell()
        .sidecar("translator")
        .map_err(|e| format!("[Sidecar] Error: {}", e))?
        .args(args)
        .spawn()
        .map_err(|e| format!("[Sidecar] Failed: {}", e))?;

    // 2. STORE THE CHILD IN STATE (Transfer Ownership)
    {
        let mut tx_guard = state.tx.lock().unwrap();
        // Move 'child' into the Mutex. DO NOT CLONE.
        *tx_guard = Some(child);
        inject_system_message(&app, SystemLogLevel::Success, "Sidecar", "Process handle saved to AppState.");
    }

    // 3. Handle Stdout (Unchanged logic, but uses updated State for O(1) history)
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes).to_string();

                    // Parse as generic JSON first to check the "type" field
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                        match json["type"].as_str() {
                            Some("info") | Some("status") => {
                                let msg = json["message"].as_str().unwrap_or("");
                                inject_system_message(&app_clone, SystemLogLevel::Info, "Sidecar", json["message"].as_str().unwrap_or(""));
                            }
                            Some("error") => {
                                let msg = json["message"].as_str().unwrap_or("");
                                inject_system_message(&app_clone, SystemLogLevel::Error, "Sidecar", json["message"].as_str().unwrap_or(""));
                            }
                            Some("result") => {
                                // 1. Try to parse as Nickname Response
                                if let Ok(nick_resp) = serde_json::from_value::<NicknameResponse>(json.clone()) {
                                    // Update History for persistence
                                    if let Some(state) = app_clone.try_state::<crate::AppState>() {
                                        let mut history = state.chat_history.lock().unwrap();
                                        if let Some(packet) = history.get_mut(&nick_resp.pid) {
                                            let original_name = packet.nickname.clone();

                                            // 2. Update only the Cache
                                            let mut cache = state.nickname_cache.lock().unwrap();
                                            cache.insert(original_name, nick_resp.romaji.clone());
                                        }
                                    }
                                    // Emit dedicated nickname event
                                    let _ = app_clone.emit("nickname-feature-event", nick_resp);
                                    continue; // Move to next line
                                }

                                // 2. Try to parse as Message Response
                                if let Ok(msg_resp) = serde_json::from_value::<MessageResponse>(json) {
                                    if let Some(state) = app_clone.try_state::<crate::AppState>() {
                                        let mut history = state.chat_history.lock().unwrap();
                                        if let Some(packet) = history.get_mut(&msg_resp.pid) {
                                            packet.translated = Some(msg_resp.translated.clone());
                                        }
                                    }
                                    // Emit dedicated translation event
                                    let _ = app_clone.emit("translation-feature-event", msg_resp);
                                }

                            }
                            Some("debug") => {
                                inject_system_message(&app_clone, SystemLogLevel::Debug, "Sidecar", json["message"].as_str().unwrap_or(""));
                            }
                            _ => inject_system_message(&app_clone, SystemLogLevel::Warning, "Sidecar", format!("Unknown JSON type: {}", line)),
                        }
                    }
                }
                // 2. CRITICAL: Listen for Stderr to see Python crashes or library errors
                CommandEvent::Stderr(error_bytes) => {
                    let err = String::from_utf8_lossy(&error_bytes);
                    if err.contains("ImportError: DLL load failed") {
                        inject_system_message(&app_clone, SystemLogLevel::Error, "Sidecar", "Missing Visual C++ Redistributable.");
                    } else if err.contains("CUDA error: out of memory") {
                        inject_system_message(&app_clone, SystemLogLevel::Error, "Sidecar", "GPU Memory Out. Try lowering the Tier.");
                    } else {
                        inject_system_message(&app_clone, SystemLogLevel::Error, "Sidecar", format!("Crash: {}", err));
                    }
                }
                _ => {}
            }
        }
        let _ = app_clone.emit("translator-status", "Disconnected");
    });
    Ok("Connected".into())
}

#[tauri::command]
pub async fn translate_nickname(
    pid: u64,
    nickname: String,
    app: AppHandle,
    state: State<'_, AppState>
) -> Result<(), String> {
    inject_system_message(&app, SystemLogLevel::Debug, "Translator", format!("Requesting nickname: {}", nickname));
    // 1. Check Backend Cache first
    {
        let cache = state.nickname_cache.lock().unwrap();
        if let Some(romaji) = cache.get(&nickname) {
            // Instant return if found
            let _ = app.emit("nickname-feature-event", NicknameResponse {
                pid,
                nickname,
                romaji: romaji.clone(),
            });
            return Ok(());
        }
    }

    let mut guard = state.tx.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        // Send a clean NicknameRequest
        let req = NicknameRequest {
            cmd: "nickname_only".into(),
            pid,
            nickname,
        };
        let msg = serde_json::to_string(&req).map_err(|e| e.to_string())? + "\n";
        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    } else { Err("Translator not initialized".into()) }
}

#[tauri::command]
pub async fn translate_message(
    text: String,
    pid: u64,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    inject_system_message(&app, SystemLogLevel::Debug, "Translator", format!("Requesting text: {}", text));

    let mut guard = state.tx.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        // Send a clean MessageRequest
        let req = MessageRequest {
            cmd: "translate".into(),
            pid,
            text,
        };
        let msg = serde_json::to_string(&req).map_err(|e| e.to_string())? + "\n";
        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    } else { Err("Translator not initialized".into()) }
}

#[tauri::command]
pub fn is_translator_running(state: tauri::State<'_, AppState>) -> bool {
    // Returns true if the child process handle exists in the Mutex
    state.tx.lock().unwrap().is_some()
}