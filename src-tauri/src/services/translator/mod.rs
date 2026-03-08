pub mod core;
pub mod processor;
pub mod server_manager;

pub use server_manager::*;

use crossbeam_channel::{unbounded, Sender};
use reqwest::blocking::Client;
use std::path::PathBuf;
use std::thread;
use tauri::{AppHandle, Emitter, Manager};

use crate::protocol::types::{ChatMessage, SystemLogLevel, TranslatorStatePayload};
use crate::{inject_system_message, kill_orphaned_servers};

use self::core::{translate_text, AI_SERVER_URL};
use self::processor::{load_dictionary, postprocess_text, preprocess_text};
use self::server_manager::{launch_ai_server, server_health_check_for_30_seconds, ServerGuard};

pub struct TranslationJob {
    pub chat: ChatMessage,
}

pub fn start_translator_worker(app: AppHandle, model_path: PathBuf) -> Sender<TranslationJob> {
    let (tx, rx) = unbounded();
    let config = crate::config::load_config(app.clone());

    thread::spawn(move || {
        inject_system_message(
            &app,
            SystemLogLevel::Info,
            "Translator",
            "Initializing HTTP AI Backend...",
        );
        emit_translator_state(&app, "Starting", "Initializing AI Backend...");

        kill_orphaned_servers(&app);

        // 1. Launch the Server
        let server_process = match launch_ai_server(&app, &model_path, &config) {
            Some(p) => p,
            None => return,
        };
        let _server_guard = ServerGuard(server_process);

        // 2. Wait for Health
        emit_translator_state(&app, "Loading Model", "Loading AI weights into VRAM...");
        if !server_health_check_for_30_seconds(&app) {
            emit_translator_state(
                &app,
                "Error",
                "AI Engine failed to start (OOM or missing model).",
            );
            return;
        }

        // 3. Setup Dependencies
        let client = Client::new();
        let dict_path = app.path().app_data_dir().unwrap().join("custom_dict.json");
        let custom_dict = load_dictionary(&dict_path);

        inject_system_message(
            &app,
            SystemLogLevel::Success,
            "Translator",
            "AI Server running! Ready for translation.",
        );
        emit_translator_state(&app, "Active", "AI Engine Ready");

        // 4. Run the pure translation loop
        while let Ok(job) = rx.recv() {
            process_translation_job(job, &client, &custom_dict, &app);
        }
    });

    tx
}

fn process_translation_job(
    job: TranslationJob,
    client: &Client,
    dict: &std::collections::HashMap<String, String>,
    app: &AppHandle,
) {
    let chat = job.chat;
    let state = app.state::<crate::AppState>();
    let nick_cache = state.nickname_cache.lock().unwrap();

    // 1. Preprocess
    let shield = preprocess_text(&chat.message, dict, Some(&nick_cache));

    // 2. HTTP Request (Blocking)
    let raw_translation = translate_text(client, AI_SERVER_URL, &shield.masked_text);

    // 3. Postprocess
    let final_str = postprocess_text(&raw_translation, &shield);

    // 4. Dispatch Side Effects
    if let Some(df_tx) = state.data_factory_tx.lock().unwrap().as_ref() {
        let _ = df_tx.send(crate::io::DataFactoryJob {
            pid: chat.pid,
            original: chat.message.clone(),
            translated: Some(final_str.clone()),
        });
    }

    if let Some(existing_chat) = state.chat_history.lock().unwrap().get_mut(&chat.pid) {
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
                        // Route 2: Mock the Translation endpoint
                        else if request.starts_with("POST /v1/chat/completions") {
                            let body = r#"{"choices": [{"message": {"content": "116 정찰 우측 은나포"}}]}"#;
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
