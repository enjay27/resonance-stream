use super::core::{server_url, set_server_port, PREFERRED_SERVER_PORT};
use crate::protocol::types::SystemLogLevel;
use crate::{inject_system_message, AI_SERVER_FILENAME, AI_SERVER_FOLDER};
use reqwest::blocking::Client;
use resonance_core::workers::pick_local_port;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// PID of the llama-server this app started (0: none). Only this process,
/// or the one recorded in the PID file by a previous run, is ever killed --
/// never other llama-server.exe instances the user may be running.
static SERVER_PID: AtomicU32 = AtomicU32::new(0);

fn pid_file(app: &AppHandle) -> Option<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .ok()?
        .join("bin")
        .join(AI_SERVER_FOLDER);
    Some(dir.join("llama-server.pid"))
}

/// Kills `pid` only if it is still a llama-server (a PID can be reused).
fn kill_server_pid(pid: u32) {
    let _ = Command::new("taskkill")
        .args([
            "/F",
            "/PID",
            &pid.to_string(),
            "/FI",
            "IMAGENAME eq llama-server.exe",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

pub struct ServerGuard {
    child: Child,
    pid_file: Option<PathBuf>,
}

impl ServerGuard {
    fn new(app: &AppHandle, child: Child) -> Self {
        let pid = child.id();
        SERVER_PID.store(pid, Ordering::SeqCst);
        let pid_file = pid_file(app);
        if let Some(path) = &pid_file {
            let _ = fs::write(path, pid.to_string());
        }
        Self { child, pid_file }
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let pid = self.child.id();
        let _ = self.child.kill();
        // A newer server may already have replaced these; only clear our own.
        let _ = SERVER_PID.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
        if let Some(path) = &self.pid_file {
            if fs::read_to_string(path).is_ok_and(|s| s.trim() == pid.to_string()) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

pub fn launch_ai_server(
    app: &AppHandle,
    model_path: &PathBuf,
    config: &crate::config::AppConfig,
) -> Option<ServerGuard> {
    let server_path = app
        .path()
        .app_data_dir()
        .unwrap()
        .join("bin")
        .join(AI_SERVER_FOLDER)
        .join(AI_SERVER_FILENAME);

    let mut server_cmd = Command::new(server_path);
    server_cmd.arg("-m").arg(model_path);
    let port = pick_local_port(PREFERRED_SERVER_PORT);
    set_server_port(port);
    server_cmd.arg("--port").arg(port.to_string());
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
    server_cmd.creation_flags(CREATE_NO_WINDOW);

    match server_cmd.spawn() {
        Ok(child) => Some(ServerGuard::new(app, child)),
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
            format!("Polling {}/health...", server_url()),
        );

        if let Ok(res) = client.get(format!("{}/health", server_url())).send() {
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
pub async fn ai_server_health_check(app: AppHandle) -> bool {
    // async: a blocking HTTP call here would freeze the window
    match reqwest::get(format!("{}/health", server_url())).await {
        Ok(res) => res.status().is_success(),
        Err(_) => {
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

/// Stops the llama-server this app started, and one left behind by a
/// previous run that crashed (recorded in the PID file).
pub fn kill_orphaned_servers(app: &AppHandle) {
    inject_system_message(
        app,
        SystemLogLevel::Info,
        "Translator",
        "Cleaning up any orphaned AI server processes...",
    );

    let current = SERVER_PID.swap(0, Ordering::SeqCst);
    if current != 0 {
        kill_server_pid(current);
    }
    if let Some(path) = pid_file(app) {
        if let Some(pid) = fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            if pid != current {
                kill_server_pid(pid);
            }
        }
        let _ = fs::remove_file(path);
    }
}
