use crate::config::*;
use crate::model_manager::*;
use crate::python_translator::*;
use crate::sniffer::*;
use indexmap::IndexMap;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

mod model_manager;
mod python_translator;
mod sniffer_logic_test;
mod sniffer;
mod packet_buffer;
mod config;


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();
            inject_system_message(handle, SystemLogLevel::Info, "Backend", "Initializing Resonance Stream...");

            let is_admin = is_elevated::is_elevated();
            inject_system_message(handle, SystemLogLevel::Info, "Backend", format!("Admin Privileges: {}", is_admin));

            if !is_admin {
                inject_system_message(handle, SystemLogLevel::Warning, "Backend", "Sniffer may fail without Admin rights.");
            }

            // 2. Spawn the Watchdog Thread
            let watchdog_app_handle = app.handle().clone();
            std::thread::spawn(move || {
                start_batch_watchdog(watchdog_app_handle);
            });

            Ok(())
        })
        .manage(AppState {
            batch_data: Arc::new((Mutex::new((vec![], 0)), Default::default())),
            sidecar_child: Mutex::new(None),
            chat_history: Mutex::new(IndexMap::new()),
            system_history: Mutex::new(VecDeque::with_capacity(200)),
            next_pid: 1.into(),
            nickname_cache: Mutex::new(std::collections::HashMap::new()),
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            check_model_status,
            download_model,
            is_translator_running,
            start_translator_sidecar,
            translate_message,
            translate_nickname,
            start_sniffer_command,
            get_chat_history,
            get_system_history,
            check_dict_update,
            sync_dictionary,
            clear_chat_history,
            set_always_on_top,
            load_config,
            save_config,
            minimize_window,
            close_window,
            open_model_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn inject_system_message<S: Into<String>>(
    app: &tauri::AppHandle,
    level: SystemLogLevel,
    source: &str,
    message: S
) {
    let msg = message.into();

    if let Some(state) = app.try_state::<AppState>() {
        let current_pid = state.next_pid.fetch_add(1, Ordering::SeqCst);

        // Map the Enum to the string expected by the frontend SystemMessage struct
        let level_str = match level {
            SystemLogLevel::Info => "info",
            SystemLogLevel::Warning => "warn",
            SystemLogLevel::Error => "error",
            SystemLogLevel::Success => "success",
            SystemLogLevel::Debug => "debug",
        };

        println!("[{}] [{}] [{:?}] {}", current_pid, source, level, msg);

        let system_message = SystemMessage {
            pid: current_pid,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            level: level_str.to_string(),
            source: source.to_string(),
            message: msg,
        };

        // Store in specialized system storage
        {
            let mut sys_hist = state.system_history.lock().unwrap();
            if sys_hist.len() >= 200 {
                sys_hist.pop_front();
            }
            sys_hist.push_back(system_message.clone());
        }

        let _ = app.emit("system-event", &system_message);
    }
}

pub fn store_and_emit(app: &tauri::AppHandle, mut packet: ChatMessage) {
    if let Some(state) = app.try_state::<AppState>() {
        let current_pid = state.next_pid.fetch_add(1, Ordering::SeqCst);
        packet.pid = current_pid;

        // Auto-populate from Backend Cache
        {
            let cache = state.nickname_cache.lock().unwrap();
            if let Some(romaji) = cache.get(&packet.nickname) {
                packet.nickname_romaji = Some(romaji.clone());
            }
        }

        // Store in HOT Storage (IndexMap)
        {
            let config = load_config(app.clone());

            let mut history = state.chat_history.lock().unwrap();
            while history.len() >= config.chat_limit && !history.is_empty() {
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

#[tauri::command]
fn minimize_window(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn close_window(window: tauri::Window) {
    let _ = window.close();
}