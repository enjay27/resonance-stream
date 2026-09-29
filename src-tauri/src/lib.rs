use indexmap::IndexMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub mod commands;
pub mod config;
pub mod events;
pub mod io;
pub mod logging;
pub mod protocol;
pub mod services;
pub mod shortcut;
pub mod tray;
pub mod window;

pub use commands::*;
pub use config::*;
pub use events::*;
pub use io::*;
pub use protocol::types::*;
pub use services::downloader::*;
pub use services::sniffer::*;
pub use services::translator::*;
pub use shortcut::*;
pub use tray::*;
pub use window::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init_logger();

    let app = tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            inject_system_message(
                &handle,
                SystemLogLevel::Info,
                "Backend",
                "Initializing Resonance Stream...",
            );

            let is_admin = is_elevated::is_elevated();
            inject_system_message(
                &handle,
                SystemLogLevel::Info,
                "Backend",
                format!("Admin Privileges: {}", is_admin),
            );

            if !is_admin {
                inject_system_message(
                    &handle,
                    SystemLogLevel::Warning,
                    "Backend",
                    "Sniffer may fail without Admin rights.",
                );
            }

            // --- CHECK CONFIG AND START AI IF NEEDED ---
            let config = load_config(handle.clone());

            update_global_tab_shortcut(
                handle.clone(),
                config.tab_switch_modifier.clone(),
                config.tab_switch_key.clone(),
            );

            let initial_tx = if config.use_translation {
                let model_path = crate::get_model_path(&handle);
                Some(crate::services::translator::start_translator_worker(
                    handle.clone(),
                    model_path,
                ))
            } else {
                None
            };

            // --- CHECK CONFIG AND START DATA LOGGING IF NEEDED ---
            let initial_df_tx = if config.archive_chat {
                Some(crate::io::start_data_factory_worker(handle.clone()))
            } else {
                None
            };

            crate::tray::setup_tray(app)?;

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
                dedup_cache: Mutex::new(std::collections::HashMap::new()),
                blocked_users: Mutex::new(config.blocked_users.clone()),
            });

            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            check_model_status,
            download_model,
            check_ai_server_status,
            download_ai_server,
            start_sniffer_command,
            get_chat_history,
            get_system_history,
            check_all_updates,
            ignore_update,
            download_app_update,
            restart_to_apply_update,
            sync_dictionary,
            get_dict_version,
            get_local_dictionary,
            save_local_dictionary,
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
            launch_translator,
            block_user_command,
            unblock_user_command,
            ai_server_health_check,
            ui_system_message,
            update_global_tab_shortcut,
            ensure_firewall_rule_command,
            restart_sniffer_command,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
            log::info!("Application closing. Cleaning AI Server...");

            // Explicitly kill the llama-server to prevent zombie processes
            #[cfg(target_os = "windows")]
            {
                services::translator::server_manager::kill_orphaned_servers(&_app_handle);
            }
        }
    });
}
