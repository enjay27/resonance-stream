use std::sync::{Arc, Mutex};
use tauri::Manager;
use crate::python_translator::*;
use crate::model_manager::*;

mod model_manager;
mod python_translator;
mod sniffer_logic_test;
mod sniffer;
mod packet_buffer;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window = app.get_window("main").unwrap();
            sniffer::start_sniffer(window); // Start the background thread
            Ok(())
        })
        .manage(AppState { tx: Mutex::new(None)})
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            check_model_status,
            download_model,
            start_translator_sidecar,
            manual_translate
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}