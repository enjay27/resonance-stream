//! Tauri commands for chat/system history and the translator toggle.

use crate::{inject_system_message, AppState, ChatMessage, SystemLogLevel, SystemMessage};
use tauri::{AppHandle, State};

#[tauri::command]
pub fn clear_chat_history(state: tauri::State<AppState>) {
    // 1. Clear Game Chat
    let mut history = state.chat_history.lock().unwrap();
    history.clear();

    // 2. Clear System Logs (Optional, but good for a full reset)
    let mut sys_history = state.system_history.lock().unwrap();
    sys_history.clear();
}

#[tauri::command]
pub fn ui_system_message(
    app: tauri::AppHandle, // Tauri prefers AppHandle passed by value in commands
    level: String,
    source: String,  // Use concrete String for frontend IPC
    message: String, // Use concrete String for frontend IPC
) {
    let sys_level = match level.to_lowercase().as_str() {
        "warn" | "warning" => SystemLogLevel::Warning,
        "error" => SystemLogLevel::Error,
        "success" => SystemLogLevel::Success,
        "debug" => SystemLogLevel::Debug,
        "trace" => SystemLogLevel::Trace,
        _ => SystemLogLevel::Info, // Default fallback
    };

    inject_system_message(&app, sys_level, &source, message);
}

#[tauri::command]
pub fn get_chat_history(state: tauri::State<AppState>) -> Vec<ChatMessage> {
    // Returns ONLY Game Chat
    let history = state.chat_history.lock().unwrap();
    history.values().cloned().collect()
}

#[tauri::command]
pub fn get_system_history(state: tauri::State<AppState>) -> Vec<SystemMessage> {
    // Change: Returns specialized SystemMessages
    let history = state.system_history.lock().unwrap();
    history.iter().cloned().collect()
}

#[tauri::command]
pub fn launch_translator(app: AppHandle, state: State<'_, AppState>) {
    // Turned ON: Start the server and store the Sender
    let model_path = crate::get_model_path(&app);
    let tx = crate::services::translator::start_translator_worker(app.clone(), model_path);
    *state.translator_tx.lock().unwrap() = Some(tx);
}
