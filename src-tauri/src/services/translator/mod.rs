pub mod core;
pub mod server_manager;

pub use server_manager::*;

use crossbeam_channel::{unbounded, Sender};
use reqwest::blocking::Client;
use std::path::PathBuf;
use std::thread;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

use crate::inject_system_message;
use crate::protocol::types::{ChatMessage, SystemLogLevel, TranslatorStatePayload};

use self::core::{server_url, translate_text};
use resonance_core::text::{postprocess_text, preprocess_text};
use resonance_core::workers::translation_is_stale;

pub struct TranslationJob {
    pub chat: ChatMessage,
    pub queued_at: Instant,
}

impl TranslationJob {
    pub fn new(chat: ChatMessage) -> Self {
        Self {
            chat,
            queued_at: Instant::now(),
        }
    }
}

pub fn start_translator_worker(app: AppHandle, model_path: PathBuf) -> Sender<TranslationJob> {
    let (tx, rx) = unbounded::<TranslationJob>();
    let config = crate::config::current_config(&app);

    thread::spawn(move || {
        inject_system_message(
            &app,
            SystemLogLevel::Info,
            "Translator",
            "Initializing HTTP AI Backend...",
        );
        emit_translator_state(&app, "Starting", "Initializing AI Backend...");

        server_manager::kill_orphaned_servers(&app);

        // 1. Launch the Server
        let _server_guard = match server_manager::launch_ai_server(&app, &model_path, &config) {
            Some(guard) => guard,
            None => return,
        };

        // 2. Wait for Health
        emit_translator_state(&app, "Loading Model", "Loading AI weights into VRAM...");
        if !server_manager::server_health_check_for_30_seconds(&app) {
            emit_translator_state(
                &app,
                "Error",
                "AI Engine failed to start (OOM or missing model).",
            );
            return;
        }

        // 3. Setup Dependencies (the dictionary lives in AppState, so a
        // sync or an edit applies to the next job without a restart)
        let client = Client::new();

        inject_system_message(
            &app,
            SystemLogLevel::Success,
            "Translator",
            "AI Server running! Ready for translation.",
        );
        emit_translator_state(&app, "Active", "AI Engine Ready");

        // 4. Run the pure translation loop
        while let Ok(job) = rx.recv() {
            if translation_is_stale(job.queued_at, Instant::now()) {
                log::debug!("[Translator] Skipped pid {}: waited too long", job.chat.pid);
                continue;
            }
            process_translation_job(job, &client, &app);
        }
    });

    tx
}

fn process_translation_job(job: TranslationJob, client: &Client, app: &AppHandle) {
    let chat = job.chat;
    let state = app.state::<crate::AppState>();

    // 1. Preprocess. The nickname lock is held only for this step: the
    // sniffer needs it for every Japanese nickname, and must not wait for
    // the HTTP round trip below.
    let dict = state.dictionary.read().clone();
    let shield = {
        let nick_cache = state.nickname_cache.lock();
        preprocess_text(&chat.message, &dict, Some(&nick_cache))
    };

    // 2. HTTP Request (Blocking)
    let raw_translation = translate_text(client, &server_url(), &shield.masked_text);

    // 3. Postprocess
    let final_str = postprocess_text(&raw_translation, &shield);

    // 4. Dispatch Side Effects
    if let Some(df_tx) = state.data_factory_tx.lock().as_ref() {
        let _ = df_tx.send(crate::io::DataFactoryJob {
            pid: chat.pid,
            original: chat.message.clone(),
            translated: Some(final_str.clone()),
        });
    }

    if let Some(existing_chat) = state.chat_history.lock().get_mut(chat.pid) {
        existing_chat.translated = Some(final_str.clone());
    }

    let _ = app.emit(
        "translation-event",
        &crate::protocol::types::TranslationResult {
            pid: chat.pid,
            translated: final_str,
        },
    );
}

pub fn emit_translator_state(app: &tauri::AppHandle, state: &str, message: &str) {
    let _ = app.emit(
        "translator-state",
        TranslatorStatePayload {
            state: state.to_string(),
            message: message.to_string(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::blocking::Client;
    use std::time::{Duration, Instant};

    #[test]
    fn test_full_translator_flow_with_mock() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        // 1. Create a mock server on a random available port
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind mock server");
        let port = listener.local_addr().unwrap().port();
        let mock_url = format!("http://127.0.0.1:{}", port);

        // 2. Spawn a thread to act as the "llama-server"
        std::thread::spawn(move || {
            // Loop to handle multiple incoming requests (health check + translation)
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    let mut buffer = [0; 4096];
                    if let Ok(bytes_read) = stream.read(&mut buffer) {
                        let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                        // Route 1: Mock the Health Check endpoint
                        if request.starts_with("GET /health") {
                            let body = r#"{"status":"ok"}"#;
                            // FIXED: Added Content-Length and Connection: close
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(), body
                            );
                            let _ = stream.write_all(response.as_bytes());
                        }
                        // Route 2: Mock llama.cpp's native /completion endpoint,
                        // which translate_text() calls (not the OpenAI-style one)
                        else if request.starts_with("POST /completion") {
                            let body = r#"{"content": " 116 정찰 우측 은나포 "}"#;
                            // FIXED: Added Content-Length and Connection: close
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(), body
                            );
                            let _ = stream.write_all(response.as_bytes());
                        }
                    }
                }
            }
        });

        let client = Client::new();

        // 3. Test the Health Check polling logic against the mock
        let mut is_ready = false;
        let start_wait = Instant::now();

        // We use a much shorter timeout (2 seconds) since the mock is instant
        while start_wait.elapsed().as_secs() < 2 {
            if let Ok(res) = client.get(format!("{}/health", mock_url)).send() {
                if res.status().is_success() {
                    is_ready = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        assert!(is_ready, "Mock server failed the health check loop!");

        // 4. Test the actual Translation pipeline against the mock
        let test_jp = "116　偵察右　銀なぽ";
        let result_ko = translate_text(&client, &mock_url, test_jp);

        assert_eq!(result_ko, "116 정찰 우측 은나포");
    }
}
