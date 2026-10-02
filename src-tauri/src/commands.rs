//! Tauri commands for chat/system history, the translator toggle and furigana.

use crate::{inject_system_message, AppState, ChatMessage, SystemLogLevel, SystemMessage};
use resonance_types::RubySpan;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn clear_chat_history(state: tauri::State<AppState>) {
    // 1. Clear Game Chat
    let mut history = state.chat_history.lock();
    history.clear();

    // 2. Clear System Logs (Optional, but good for a full reset)
    let mut sys_history = state.system_history.lock();
    sys_history.clear();
}

#[tauri::command]
pub fn ui_system_message(
    app: tauri::AppHandle, // Tauri prefers AppHandle passed by value in commands
    level: String,
    source: String,  // Use concrete String for frontend IPC
    message: String, // Use concrete String for frontend IPC
) {
    inject_system_message(&app, SystemLogLevel::parse(&level), &source, message);
}

#[tauri::command]
pub fn get_chat_history(state: tauri::State<AppState>) -> Vec<ChatMessage> {
    // Returns ONLY Game Chat
    let history = state.chat_history.lock();
    history.values().cloned().collect()
}

#[tauri::command]
pub fn get_system_history(state: tauri::State<AppState>) -> Vec<SystemMessage> {
    // Change: Returns specialized SystemMessages
    let history = state.system_history.lock();
    history.iter().cloned().collect()
}

/// The last state each service reported. The UI asks once its listeners are
/// registered: states emitted before that (the translator starts with the
/// app) would otherwise be lost.
#[tauri::command]
pub fn get_service_states(state: tauri::State<AppState>) -> crate::ServiceStates {
    state.service_states.lock().clone()
}

#[tauri::command]
pub fn launch_translator(app: AppHandle, state: State<'_, AppState>) {
    // Idempotent: a translator that is already running is left alone.
    let mut slot = state.translator_tx.lock();
    if slot.is_some() {
        return;
    }
    let model_path = crate::get_model_path(&app);
    let tx = crate::services::translator::start_translator_worker(app.clone(), model_path);
    *slot = Some(tx);
}

/// Furigana for Japanese lines, one list of spans per line, in order. The
/// analysis itself is `resonance_core::furigana`; it takes microseconds a line,
/// so it runs right here on the async runtime.
#[tauri::command]
pub async fn annotate_furigana(texts: Vec<String>) -> Vec<Vec<RubySpan>> {
    texts
        .iter()
        .map(|text| resonance_core::furigana::annotate(text))
        .collect()
}
