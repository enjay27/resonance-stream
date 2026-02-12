use crate::model_manager::*;
use crate::python_translator::*;
use crate::sniffer::*;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use indexmap::IndexMap;
use tauri::{Emitter, Manager};

mod model_manager;
mod python_translator;
mod sniffer_logic_test;
mod sniffer;
mod packet_buffer;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { tx: Mutex::new(None), chat_history: Mutex::new(IndexMap::new()), next_pid: 1.into() })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            check_model_status,
            download_model,
            start_translator_sidecar,
            manual_translate,
            start_sniffer_command,
            get_chat_history,
            check_dict_update,
            sync_dictionary
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn inject_system_message<S: Into<String>>(app: &tauri::AppHandle, message: S) {
    let msg = message.into();
    println!("[System] {}", msg);

    let sys_packet = ChatPacket {
        channel: "SYSTEM".into(),
        nickname: "SYSTEM".into(),
        message: msg,
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
        ..Default::default()
    };

    // Use the extracted logic
    store_and_emit(app, sys_packet);
}

pub fn store_and_emit(app: &tauri::AppHandle, mut packet: ChatPacket) {
    if let Some(state) = app.try_state::<AppState>() {
        // 1. Assign and Increment the Internal PID
        let current_pid = state.next_pid.fetch_add(1, Ordering::SeqCst);
        packet.pid = current_pid;

        // 2. FIFO Logic: Update the IndexMap
        {
            let mut history = state.chat_history.lock().unwrap();
            if history.len() >= 1000 {
                // O(n) but negligible for 1000 small structs
                history.shift_remove_index(0);
            }
            // O(1) Insertion
            history.insert(packet.pid, packet.clone());
        }

        // 3. Emit to UI (Unified channel name)
        let _ = app.emit("packet-event", &packet);
    }
}