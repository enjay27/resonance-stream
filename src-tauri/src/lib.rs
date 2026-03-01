use std::io::Write;
use crate::config::*;
use indexmap::IndexMap;
use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use env_logger::fmt::style::{AnsiColor, Color, Style};
use lazy_static::lazy_static;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_shell::ShellExt;

lazy_static! {
    // Stores: (Message Fingerprint, Arrival Time)
    static ref CHAT_DEDUPE_CACHE: Mutex<VecDeque<(u64, Instant)>> = Mutex::new(VecDeque::new());
}

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
            let handle = app.handle().clone();
            inject_system_message(&handle, SystemLogLevel::Info, "Backend", "Initializing Resonance Stream...");

            let is_admin = is_elevated::is_elevated();
            inject_system_message(&handle, SystemLogLevel::Info, "Backend", format!("Admin Privileges: {}", is_admin));

            if !is_admin {
                inject_system_message(&handle, SystemLogLevel::Warning, "Backend", "Sniffer may fail without Admin rights.");
            }

            // --- CHECK CONFIG AND START AI IF NEEDED ---
            let config = load_config(handle.clone());
            let initial_tx = if config.use_translation {
                let model_path = crate::get_model_path(&handle);
                Some(crate::services::translator::start_translator_worker(handle.clone(), model_path))
            } else {
                None
            };

            // --- CHECK CONFIG AND START DATA LOGGING IF NEEDED ---
            let initial_df_tx = if config.archive_chat {
                Some(crate::io::start_data_factory_worker(handle.clone()))
            } else {
                None
            };

            let toggle_i = MenuItem::with_id(app, "toggle_click_through", "클릭 관통 (Click-Through): OFF", true, None::<&str>)?;
            let top_i = MenuItem::with_id(app, "toggle_always_on_top", "항상 위에 표시 (Always on Top): OFF", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "앱 열기 (Open App)", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "종료 (Quit)", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&top_i, &toggle_i, &show_i, &quit_i])?;

            // Store the items in Tauri's managed state so the command can mutate them later
            app.manage(TrayMenuState {
                click_through: toggle_i.clone(),
                always_on_top: top_i.clone(),
            });

            let _tray = TrayIconBuilder::new()
                .tooltip("Resonance Stream")
                .icon(app.default_window_icon().unwrap().clone()) // Uses icon from tauri.conf.json
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "toggle_always_on_top" => {
                            let _ = app.emit("tray-toggle-always-on-top", ());
                        }
                        "toggle_click_through" => {
                            // Tell the frontend to flip the toggle and update the window
                            let _ = app.emit("tray-toggle-click-through", ());
                        }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // Initialize State INSIDE setup so we have access to the App context
            app.manage(AppState {
                batch_data: Arc::new((Mutex::new((vec![], 0)), Default::default())),
                chat_history: Mutex::new(IndexMap::new()),
                system_history: Mutex::new(VecDeque::with_capacity(200)),
                next_pid: 1.into(),
                nickname_cache: Mutex::new(std::collections::HashMap::new()),
                translator_tx: Mutex::new(initial_tx),
                data_factory_tx: Mutex::new(initial_df_tx),
                sniffer_tx: Mutex::new(None),
            });

            Ok(())
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
            open_browser,
            get_network_interfaces,
            set_click_through,
            update_tray_menu,
            launch_translator
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
            log::info!("Application closing. Cleaning up Firewall Rules & AI Server...");

            // 1. Remove the firewall rule
            services::sniffer::remove_firewall_rule();

            // 2. Explicitly kill the llama-server to prevent zombie processes
            #[cfg(target_os = "windows")]
            {
                kill_orphaned_servers(&_app_handle);
            }
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
    let fingerprint = generate_message_fingerprint(&packet);
    let now = Instant::now();

    {
        let mut cache = CHAT_DEDUPE_CACHE.lock().unwrap();

        // 1. Prune old messages from the sliding window (e.g., older than 2 seconds)
        while let Some(&(_, time)) = cache.front() {
            if now.duration_since(time) > Duration::from_secs(2) {
                cache.pop_front();
            } else {
                break; // VecDeque is ordered by time, so we can stop here
            }
        }

        // 2. Check if this exact message was already processed
        if cache.iter().any(|(hash, _)| *hash == fingerprint) {
            // Silently drop the duplicate packet from the second client
            return;
        }

        // 3. Not a duplicate, add it to the cache
        cache.push_back((fingerprint, now));
    }

    if let Some(state) = app.try_state::<AppState>() {

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

// Helper to generate a unique fingerprint for the chat message
fn generate_message_fingerprint(packet: &ChatMessage) -> u64 {
    let mut hasher = DefaultHasher::new();

    // If your server provides a truly unique sequence_id for every message,
    // you only need to hash that. Otherwise, hash the combination of sender, text, and time:
    packet.uid.hash(&mut hasher);
    packet.message.hash(&mut hasher);
    packet.timestamp.hash(&mut hasher);

    hasher.finish()
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

#[tauri::command]
fn set_click_through(window: tauri::Window, enabled: bool) {
    let _ = window.set_ignore_cursor_events(enabled);
}

#[tauri::command]
fn update_tray_menu(state: tauri::State<TrayMenuState>, click_through: bool, always_on_top: bool) {
    let ct_text = if click_through {
        "클릭 관통 (Click-Through): ON"
    } else {
        "클릭 관통 (Click-Through): OFF"
    };
    let _ = state.click_through.set_text(ct_text);

    let aot_text = if always_on_top {
        "항상 위에 표시 (Always on Top): ON"
    } else {
        "항상 위에 표시 (Always on Top): OFF"
    };
    let _ = state.always_on_top.set_text(aot_text);
}

fn kill_orphaned_servers(app: &AppHandle) {
    inject_system_message(app, SystemLogLevel::Info, "Translator", "Cleaning up any orphaned AI server processes...");

    // Uses Windows taskkill to forcefully close any dangling llama-server.exe instances
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "llama-server.exe"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW so it doesn't flash a cmd prompt
        .output(); // .output() waits for the command to finish
}

#[tauri::command]
fn launch_translator(app: AppHandle, state: State<'_, AppState>) {
    // Turned ON: Start the server and store the Sender
    let model_path = crate::get_model_path(&app);
    let tx = crate::services::translator::start_translator_worker(app.clone(), model_path);
    *state.translator_tx.lock().unwrap() = Some(tx);
}