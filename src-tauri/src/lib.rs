use crate::model_manager::*;
use crate::python_translator::*;
use crate::sniffer::*;
use crate::config::*;
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
mod config;


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            tx: Mutex::new(None),
            chat_history: Mutex::new(IndexMap::new()),
            system_history: Mutex::new(VecDeque::with_capacity(200)),
            next_pid: 1.into()
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            check_model_status,
            download_model,
            start_translator_sidecar,
            manual_translate,
            start_sniffer_command,
            get_chat_history,
            get_system_history,
            check_dict_update,
            sync_dictionary,
            clear_chat_history,
            set_always_on_top,
            load_config,
            save_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn inject_system_message<S: Into<String>>(app: &tauri::AppHandle, message: S) {
    let msg = message.into();
    println!("[System] {}", msg);

    if let Some(state) = app.try_state::<AppState>() {
        // 1. Create Packet
        // System logs don't strictly need PIDs for translation,
        // but we assign one for unique keys in React/Leptos loops.
        let current_pid = state.next_pid.fetch_add(1, Ordering::SeqCst);

        let sys_packet = ChatPacket {
            pid: current_pid,
            channel: "SYSTEM".into(),
            nickname: "SYSTEM".into(),
            message: msg,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            ..Default::default()
        };

        // 2. Store in COLD Storage (VecDeque)
        {
            let mut sys_hist = state.system_history.lock().unwrap();
            if sys_hist.len() >= 200 {
                sys_hist.pop_front(); // Remove oldest log
            }
            sys_hist.push_back(sys_packet.clone());
        }

        // 3. Emit "system-event" (Distinct from packet-event)
        let _ = app.emit("system-event", &sys_packet);
    }
}

pub fn store_and_emit(app: &tauri::AppHandle, mut packet: ChatPacket) {
    if let Some(state) = app.try_state::<AppState>() {
        let current_pid = state.next_pid.fetch_add(1, Ordering::SeqCst);
        packet.pid = current_pid;

        // Store in HOT Storage (IndexMap)
        {
            let mut history = state.chat_history.lock().unwrap();
            if history.len() >= 1000 {
                history.shift_remove_index(0);
            }
            history.insert(packet.pid, packet.clone());
        }

        // Emit "packet-event" for Game Chat
        let _ = app.emit("packet-event", &packet);
    }
}

#[tauri::command]
fn clear_chat_history(state: tauri::State<AppState>) {
    // 1. Clear Game Chat
    let mut history = state.chat_history.lock().unwrap();
    history.clear();

    // 2. Clear System Logs (Optional, but good for a full reset)
    let mut sys_history = state.system_history.lock().unwrap();
    sys_history.clear();
}

#[tauri::command]
fn set_always_on_top(window: tauri::Window, on_top: bool) {
    // This simple method toggles the window state
    let _ = window.set_always_on_top(on_top);
}