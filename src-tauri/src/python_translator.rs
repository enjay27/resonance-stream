use std::fs::OpenOptions;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use crate::sniffer::{AppState, ChatMessage, SystemLogLevel};
use crate::{inject_system_message, model_manager};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use crate::config::load_config;
use std::io::Write;
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

#[derive(Serialize)]
pub struct BatchMessageRequest {
    pub cmd: String, // "batch_translate" or "translate_and_save"
    pub messages: Vec<MessageRequest>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TranslationResult {
    pub pid: u64,
    pub translated: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchMessageResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub results: Vec<TranslationResult>,
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
        let mut tx_guard = state.sidecar_child.lock().unwrap();
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
                                if let Ok(msg_resp) = serde_json::from_value::<MessageResponse>(json.clone()) {
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
                            Some("batch_result") => {
                                if let Ok(response) = serde_json::from_value::<BatchMessageResponse>(json.clone()) {
                                    if response.msg_type == "batch_result" {
                                        let st = app_clone.state::<AppState>();
                                        let history = st.chat_history.lock().unwrap();

                                        for result in response.results {
                                            // 2. Match the result PID to the original message in history
                                            if let Some(original) = history.get(&result.pid) {
                                                // 3. Save to AppData/dataset_raw.jsonl
                                                let _ = crate::save_to_data_factory(
                                                    &app_clone,
                                                    result.pid,
                                                    &original.message,
                                                    &result.translated
                                                );
                                            }

                                            // 4. Emit to UI for real-time update
                                            let _ = app_clone.emit("translation-feature-event", MessageResponse {
                                                pid: result.pid,
                                                translated: result.translated,
                                            });
                                        }
                                    }
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

    let mut guard = state.sidecar_child.lock().unwrap();
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
    // 1. Log the request in the System tab for debugging
    inject_system_message(&app, SystemLogLevel::Debug, "Translator", format!("Buffering PID {}: {}", pid, text));

    let (lock, cvar) = &*state.batch_data;

    // Create a temporary variable to hold the batch if we need to flush
    let mut batch_to_send: Option<Vec<MessageRequest>> = None;

    // 1. Lock scope: Add message and check size
    {
        let mut data = lock.lock().unwrap();
        data.0.push(MessageRequest { cmd: "translate".into(), pid, text });
        data.1 = now_ms(); // Update activity timestamp

        if data.0.len() >= 5 {
            // Take the messages out of the buffer for flushing
            batch_to_send = Some(std::mem::take(&mut data.0));
            data.1 = 0; // Reset timer
        } else {
            // Size not reached: Notify watchdog to start/reset the 1s timer
            cvar.notify_one();
        }
    } // <--- MutexGuard is dropped here automatically!

    // 2. Async scope: Perform the flush if needed
    if let Some(batch) = batch_to_send {
        inject_system_message(&app, SystemLogLevel::Debug, "Translator", "Batch request by limit size");
        let config = load_config(app.clone());
        let should_save = config.is_debug && config.archive_chat;

        // Now it's safe to .await because the lock is gone
        translate_batch(batch, should_save, state).await?;
    }

    Ok(())
}

#[tauri::command]
pub async fn translate_batch(
    messages: Vec<MessageRequest>,
    save_to_dataset: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut guard = state.sidecar_child.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        // Use the optimized "translate_and_save" command for the Data Factory
        let cmd = if save_to_dataset { "translate_and_save" } else { "batch_translate" };

        let req = BatchMessageRequest {
            cmd: cmd.into(),
            messages,
        };

        let msg = serde_json::to_string(&req).map_err(|e| e.to_string())? + "\n";
        child.write(msg.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Translator sidecar not active".into())
    }
}

pub fn start_batch_watchdog(app: tauri::AppHandle) {
    let state = app.state::<AppState>();
    let (lock, cvar) = &*state.batch_data;

    loop {
        let mut data = lock.lock().unwrap();

        // 1. If buffer is empty, sleep indefinitely until first message arrives
        while data.0.is_empty() {
            data = cvar.wait(data).unwrap();
        }

        // 2. Calculate remaining time for the 1s reactive window
        let now = now_ms();
        let elapsed = now.saturating_sub(data.1);
        let timeout = std::time::Duration::from_millis(1000u64.saturating_sub(elapsed));

        // 3. Sleep until timeout OR notify_one() is called again
        let (mut data, result) = cvar.wait_timeout(data, timeout).unwrap();

        // 4. Timer expired: Flush the stale batch
        if result.timed_out() && !data.0.is_empty() {
            inject_system_message(&app, SystemLogLevel::Debug, "Translator-Watchdog", "Batch translate by Timeout");
            let batch = std::mem::take(&mut data.0);
            data.1 = 0;
            drop(data);

            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                let config = load_config(app_clone.clone());
                let st = app_clone.state::<AppState>();
                let _ = translate_batch(batch, config.is_debug && config.archive_chat, st).await;
            });
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[tauri::command]
pub fn is_translator_running(state: tauri::State<'_, AppState>) -> bool {
    // Returns true if the child process handle exists in the Mutex
    state.sidecar_child.lock().unwrap().is_some()
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