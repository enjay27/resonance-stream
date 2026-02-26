use std::io::Write;
use crate::config::*;
use indexmap::IndexMap;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use env_logger::fmt::style::{AnsiColor, Color, Style};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::ShellExt;

pub mod config;
pub mod io;
pub mod packet_buffer;

pub mod protocol;
pub mod services;

pub use protocol::parser::*;
pub use protocol::types::*;
pub use services::downloader::*;
pub use services::sniffer::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::builder()
        .format(|buf, record| {
            let target = record.target();
            let short_target = target
                .strip_prefix("resonance_stream_lib::")
                .unwrap_or(target);

            // 1. Get the default ANSI style for the log level (Info=Green, Warn=Yellow, etc.)
            let level_style = buf.default_level_style(record.level());

            // 2. Create a custom style for the target name using the new API
            let target_style = Style::new()
                .fg_color(Some(AnsiColor::Cyan.into())) // Set text to Cyan
                .dimmed();                              // Make it slightly darker

            // 3. Apply the styles using the 0.11 `{style}text{style:#}` pattern
            writeln!(
                buf,
                "[{timestamp} {level_style}{level}{level_style:#} {target_style}{target}{target_style:#}] {message}",
                timestamp = buf.timestamp(),
                level_style = level_style,   // Turns level color ON
                level = record.level(),
                // {level_style:#} magically turns the color OFF
                target_style = target_style, // Turns target color ON
                target = short_target,
                // {target_style:#} turns target color OFF
                message = record.args()
            )
        })
        .filter_level(log::LevelFilter::Warn) // Keep other crates quiet
        .filter_module("resonance_stream_lib", log::LevelFilter::Trace) // Show your debugs
        .init();

    let app = tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();
            inject_system_message(handle, SystemLogLevel::Info, "Backend", "Initializing Resonance Stream...");

            let is_admin = is_elevated::is_elevated();
            inject_system_message(handle, SystemLogLevel::Info, "Backend", format!("Admin Privileges: {}", is_admin));

            if !is_admin {
                inject_system_message(handle, SystemLogLevel::Warning, "Backend", "Sniffer may fail without Admin rights.");
            }

            Ok(())
        })
        .manage(AppState {
            batch_data: Arc::new((Mutex::new((vec![], 0)), Default::default())),
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
            check_ai_server_status,
            download_ai_server,
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
            open_app_data_folder,
            export_chat_log,
            open_browser
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
            log::info!("Application closing. Cleaning up Firewall Rules...");
            services::sniffer::remove_firewall_rule();
        }
    });
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
            SystemLogLevel::Trace => "trace",
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

#[tauri::command]
fn get_chat_history(state: tauri::State<AppState>) -> Vec<ChatMessage> {
    // Returns ONLY Game Chat
    let history = state.chat_history.lock().unwrap();
    history.values().cloned().collect()
}

#[tauri::command]
fn get_system_history(state: tauri::State<AppState>) -> Vec<SystemMessage> {
    // Change: Returns specialized SystemMessages
    let history = state.system_history.lock().unwrap();
    history.iter().cloned().collect()
}