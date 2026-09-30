pub mod core;
pub mod server_manager;

pub use server_manager::*;

use crossbeam_channel::{unbounded, Receiver, Sender};
use reqwest::blocking::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

use crate::inject_system_message;
use crate::protocol::types::{ChatMessage, SystemLogLevel};

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

/// Identifies the translator worker that is current. A worker that was
/// stopped or replaced while it was still starting up sees the number move
/// and stays silent instead of reporting on a server that is not its own.
static WORKER_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Marks every running or starting worker as superseded (call when the
/// translator is stopped or its sender dropped).
pub fn retire_translator_workers() {
    WORKER_GENERATION.fetch_add(1, Ordering::SeqCst);
}

pub fn start_translator_worker(app: AppHandle, model_path: PathBuf) -> Sender<TranslationJob> {
    let (tx, rx) = unbounded::<TranslationJob>();
    let generation = WORKER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let is_current = move || WORKER_GENERATION.load(Ordering::SeqCst) == generation;
    let config = crate::config::current_config(&app);

    thread::spawn(move || {
        inject_system_message(
            &app,
            SystemLogLevel::Info,
            "Translator",
            "Initializing HTTP AI Backend...",
        );
        emit_translator_state(&app, "Starting", "Initializing AI Backend...");

        if !is_current() {
            return;
        }
        server_manager::kill_orphaned_servers(&app);

        // 1. Launch the Server
        let _server_guard = match server_manager::launch_ai_server(&app, &model_path, &config) {
            Some(guard) => guard,
            None => return,
        };

        // 2. Wait for Health
        emit_translator_state(&app, "Loading Model", "Loading AI weights into VRAM...");
        let healthy = server_manager::server_health_check_for_30_seconds(&app, &is_current);
        if !is_current() {
            return; // superseded while loading: the guard drops and kills our server
        }
        if !healthy {
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

        // 4. Once per start: what the ledger says was missed
        catch_up(&app, &rx, &client, &is_current);
        if !is_current() {
            return;
        }
        emit_translator_state(&app, "Active", "AI Engine Ready");

        // 5. Run the pure translation loop
        while let Ok(job) = rx.recv() {
            process_live_job(job, &client, &app);
        }
    });

    tx
}

/// A job from the sniffer: skipped when it waited too long (it stays owed
/// in the ledger, so the next translator start catches it up).
fn process_live_job(job: TranslationJob, client: &Client, app: &AppHandle) {
    if translation_is_stale(job.queued_at, Instant::now()) {
        log::debug!("[Translator] Skipped pid {}: waited too long", job.chat.pid);
        archive_chat(app, &job.chat);
        return;
    }
    process_translation_job(job, client, app);
}

/// Translates, once per translator start, the Japanese messages of this run
/// that the ledger still owes: the newest `translation_catch_up_limit` of
/// them, oldest first, read from the in-memory history. Jobs queued while
/// the server loaded are dropped first -- their messages are in the ledger.
/// Live messages arriving meanwhile go before the next catch-up item.
fn catch_up(
    app: &AppHandle,
    rx: &Receiver<TranslationJob>,
    client: &Client,
    is_current: &dyn Fn() -> bool,
) {
    let state = app.state::<crate::AppState>();
    while rx.try_recv().is_ok() {}

    let limit = crate::config::current_config(app).translation_catch_up_limit;
    let pids = state.translation_ledger.lock().catch_up(limit);
    let owed: Vec<ChatMessage> = {
        let history = state.chat_history.lock();
        let mut ledger = state.translation_ledger.lock();
        pids.into_iter()
            .filter_map(|pid| match history.get(pid) {
                Some(chat) if chat.translated.is_none() => Some(chat.clone()),
                _ => {
                    ledger.settle(pid); // gone from history, or translated already
                    None
                }
            })
            .collect()
    };
    if owed.is_empty() {
        return;
    }

    let total = owed.len();
    inject_system_message(
        app,
        SystemLogLevel::Info,
        "Translator",
        format!("Catching up {} missed message(s)...", total),
    );
    for (done, chat) in owed.into_iter().enumerate() {
        if !is_current() {
            return;
        }
        emit_translator_state(app, "Catching Up", &format!("{}/{}", done + 1, total));
        while let Ok(job) = rx.try_recv() {
            process_live_job(job, client, app);
        }
        process_translation_job(TranslationJob::new(chat), client, app);
    }
    let still_owed = state.translation_ledger.lock().len();
    inject_system_message(
        app,
        SystemLogLevel::Info,
        "Translator",
        format!("Catch-up done ({} still untranslated).", still_owed),
    );
}

/// Queues `chat` for the archive, as it is (untranslated), when archiving is
/// on and its channel is archived.
pub fn archive_chat(app: &AppHandle, chat: &ChatMessage) {
    let state = app.state::<crate::AppState>();
    if !state.config.read().archive_chat || !crate::io::archives_channel(app, &chat.channel) {
        return;
    }
    let df_tx = state.data_factory_tx.lock().clone();
    if let Some(df_tx) = df_tx {
        let _ = df_tx.send(crate::io::DataFactoryJob { chat: chat.clone() });
    }
}

fn process_translation_job(job: TranslationJob, client: &Client, app: &AppHandle) {
    let chat = job.chat;
    let state = app.state::<crate::AppState>();

    // Translated already (a catch-up item that was also queued live) or not
    // a message of this run's ledger: nothing is owed.
    if !state.translation_ledger.lock().is_owed(chat.pid) {
        return;
    }

    // 1. Preprocess. The nickname lock is held only for this step: the
    // sniffer needs it for every Japanese nickname, and must not wait for
    // the HTTP round trip below.
    let dict = state.dictionary.read().clone();
    let shield = {
        let nick_cache = state.nickname_cache.lock();
        preprocess_text(&chat.message, &dict, Some(&nick_cache))
    };

    // 2. HTTP Request (Blocking)
    let raw_translation = match translate_text(client, &server_url(), &shield.masked_text) {
        Ok(text) => text,
        Err(reason) => {
            // No translation: the row stays as it is, and the original is
            // still archived.
            log::warn!("[Translator] pid {}: {}", chat.pid, reason);
            archive_chat(app, &chat);
            return;
        }
    };

    // 3. Postprocess
    let final_str = postprocess_text(&raw_translation, &shield);

    // 4. Dispatch Side Effects
    let archive = state.data_factory_tx.lock().clone();
    if let Some(df_tx) = archive.filter(|_| crate::io::archives_channel(app, &chat.channel)) {
        let mut archived = chat.clone();
        archived.translated = Some(final_str.clone());
        let _ = df_tx.send(crate::io::DataFactoryJob { chat: archived });
    }

    if let Some(existing_chat) = state.chat_history.lock().get_mut(chat.pid) {
        existing_chat.translated = Some(final_str.clone());
    }
    state.translation_ledger.lock().settle(chat.pid);

    let _ = app.emit(
        "translation-event",
        &crate::protocol::types::TranslationResult {
            pid: chat.pid,
            translated: final_str,
        },
    );
}

/// Records the state (for `get_service_states`) and emits it. The lock is
/// held across the emit so events leave in `seq` order.
pub fn emit_translator_state(app: &tauri::AppHandle, state: &str, message: &str) {
    let Some(app_state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let mut states = app_state.service_states.lock();
    let payload = states.set_translator(state, message);
    let _ = app.emit("translator-state", payload);
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
        let result_ko = translate_text(&client, &mock_url, test_jp).unwrap();

        assert_eq!(result_ko, "116 정찰 우측 은나포");
    }
}
