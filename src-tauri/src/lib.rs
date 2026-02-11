use crate::model_manager::*;
use crate::python_translator::*;
use crate::sniffer::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

mod model_manager;
mod python_translator;
mod sniffer_logic_test;
mod sniffer;
mod packet_buffer;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { tx: Mutex::new(None), chat_history: Mutex::new(VecDeque::new()) })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            check_model_status,
            download_model,
            start_translator_sidecar,
            manual_translate,
            start_sniffer_command,
            get_chat_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn inject_system_message<S: Into<String>>(window: &tauri::Window, message: S) {
    let sys_packet = ChatPacket {
        channel: "SYSTEM".into(),
        nickname: "SYSTEM".into(),
        message: message.into(),
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
        ..Default::default()
    };
    let _ = window.emit("new-chat-message", &sys_packet);
}