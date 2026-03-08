use super::core::AI_SERVER_URL;
use crate::protocol::types::SystemLogLevel;
use crate::{inject_system_message, AI_SERVER_FILENAME, AI_SERVER_FOLDER};
use reqwest::blocking::Client;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

pub struct ServerGuard(pub Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

pub fn launch_ai_server(
    app: &AppHandle,
    model_path: &PathBuf,
    config: &crate::config::AppConfig,
) -> Option<Child> {
    let server_path = app
        .path()
        .app_data_dir()
        .unwrap()
        .join("bin")
        .join(AI_SERVER_FOLDER)
        .join(AI_SERVER_FILENAME);

    let mut server_cmd = Command::new(server_path);
    server_cmd.arg("-m").arg(model_path);
    server_cmd.arg("--port").arg("8080");
    server_cmd.arg("--log-disable");

    let gpu_layers = if config.compute_mode.to_lowercase() == "gpu" {
        match config.tier.to_lowercase().as_str() {
            "low" => "12",
            "middle" => "24",
            "high" => "32",
            "very high" => "99",
            _ => "24",
        }
    } else {
        "0"
    };

    server_cmd.args([
        "-ngl",
        gpu_layers,
        "-c",
        "1536",
        "-b",
        "64",
        "-ub",
        "64",
        "-t",
        "4",
        "--parallel",
        "1",
    ]);
    server_cmd.creation_flags(0x08000000);

    match server_cmd.spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            let err_msg = format!("Failed to start llama-server.exe. ({})", e);
            inject_system_message(app, SystemLogLevel::Error, "Translator", &err_msg);
            super::emit_translator_state(
                app,
                "Error",
                &format!("Failed to start llama-server.exe. ({})", e),
            );
            None
        }
    }
}

pub fn server_health_check_for_30_seconds(app: &AppHandle) -> bool {
    let client = Client::new();
    let start_wait = Instant::now();

    inject_system_message(
        app,
        SystemLogLevel::Info,
        "Translator",
        "Waiting for AI Engine to warm up...",
    );

    while start_wait.elapsed().as_secs() < 30 {
        inject_system_message(
            app,
            SystemLogLevel::Trace,
            "Translator",
            format!("Polling {}/health...", AI_SERVER_URL),
        );

        if let Ok(res) = client.get(format!("{}/health", AI_SERVER_URL)).send() {
            if res.status().is_success() {
                inject_system_message(
                    app,
                    SystemLogLevel::Trace,
                    "Translator",
                    format!(
                        "Health check passed after {}ms",
                        start_wait.elapsed().as_millis()
                    ),
                );
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }

    inject_system_message(
        app,
        SystemLogLevel::Error,
        "Translator",
        "AI Engine failed to initialize within 30s.",
    );
    false
}

#[tauri::command]
pub fn ai_server_health_check(app: AppHandle) -> bool {
    let client = Client::new();
    match client.get(format!("{}/health", AI_SERVER_URL)).send() {
        Ok(res) => res.status().is_success(),
        Err(e) => {
            inject_system_message(
                &app,
                SystemLogLevel::Error,
                "Translator",
                "AI Engine is unavailable.",
            );
            false
        }
    }
}
