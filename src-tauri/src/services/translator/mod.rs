pub mod core;
pub mod server_manager;

pub use server_manager::*;

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use reqwest::blocking::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::inject_system_message;
use crate::protocol::types::{ChatMessage, SystemLogLevel};

use self::core::{server_url, translate_text};
use resonance_core::text::{postprocess_text, preprocess_text};
use resonance_core::workers::{translation_is_stale, ServerSupervisor, SupervisorAction};

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

        // The dictionary lives in AppState, so a sync or an edit applies to
        // the next job without a restart.
        let client = Client::new();
        let mut supervisor = ServerSupervisor::default();

        // One pass per llama-server: a server that crashed or hung is
        // replaced (with backoff) until the supervisor gives up.
        loop {
            if !is_current() {
                return;
            }
            server_manager::kill_orphaned_servers(&app);

            // 1. Launch the Server
            let Some(mut server) = server_manager::launch_ai_server(&app, &model_path, &config)
            else {
                return;
            };

            // 2. Wait for Health
            emit_translator_state(&app, "Loading Model", "Loading AI weights into VRAM...");
            let started = server_manager::wait_for_server(&app, &mut server, &is_current);
            if !is_current() {
                return; // superseded while loading: the guard drops and kills our server
            }
            if let Err(reason) = started {
                inject_system_message(&app, SystemLogLevel::Error, "Translator", &reason);
                emit_translator_state(&app, "Error", &reason);
                drain_untranslated(&app, &rx);
                return;
            }

            inject_system_message(
                &app,
                SystemLogLevel::Success,
                "Translator",
                "AI Server running! Ready for translation.",
            );

            let mut run = ServerRun {
                app: &app,
                client: &client,
                server: &mut server,
                supervisor: &mut supervisor,
            };

            // 3. Once per server: what the ledger says was missed
            let mut outcome = catch_up(&mut run, &rx, &is_current);
            if outcome == Some(SupervisorAction::Continue) {
                if !is_current() {
                    return;
                }
                emit_translator_state(&app, "Active", "AI Engine Ready");
                // 4. The translation loop
                outcome = serve(&mut run, &rx);
            }

            match outcome {
                None | Some(SupervisorAction::Continue) => return, // stopped
                Some(SupervisorAction::Restart(wait)) => {
                    let msg = format!("AI Engine stopped. Restarting in {}s...", wait.as_secs());
                    inject_system_message(&app, SystemLogLevel::Warning, "Translator", &msg);
                    emit_translator_state(&app, "Restarting", &msg);
                    drop(server); // kills a hung server before the wait
                    if !sleep_while(wait, &is_current) {
                        return;
                    }
                }
                Some(SupervisorAction::GiveUp) => {
                    let msg = "AI Engine keeps stopping. Turn translation off and on to retry.";
                    inject_system_message(&app, SystemLogLevel::Error, "Translator", msg);
                    emit_translator_state(&app, "Error", msg);
                    drop(server);
                    drain_untranslated(&app, &rx);
                    return;
                }
            }
        }
    });

    tx
}

/// How often an idle worker checks that its server is still running.
const IDLE_CHECK: Duration = Duration::from_secs(5);

/// One running llama-server and what decides its fate.
struct ServerRun<'a> {
    app: &'a AppHandle,
    client: &'a Client,
    server: &'a mut server_manager::ServerGuard,
    supervisor: &'a mut ServerSupervisor,
}

impl ServerRun<'_> {
    /// Translates `job` and says what to do next: a failed job whose
    /// server exited, or the last of several failures in a row, restarts it.
    fn translate(&mut self, job: TranslationJob) -> SupervisorAction {
        match process_translation_job(job, self.client, self.app) {
            JobResult::Done => {
                self.supervisor.on_job_ok();
                SupervisorAction::Continue
            }
            JobResult::Skipped => SupervisorAction::Continue,
            JobResult::Failed => match self.server.exit_status() {
                Some(status) => self.server_exited(&status),
                None => self.supervisor.on_job_failed(Instant::now()),
            },
        }
    }

    fn live(&mut self, job: TranslationJob) -> SupervisorAction {
        if translation_is_stale(job.queued_at, Instant::now()) {
            log::debug!("[Translator] Skipped pid {}: waited too long", job.chat.pid);
            archive_chat(self.app, &job.chat);
            return SupervisorAction::Continue;
        }
        self.translate(job)
    }

    fn server_exited(&mut self, status: &str) -> SupervisorAction {
        inject_system_message(
            self.app,
            SystemLogLevel::Warning,
            "Translator",
            format!("llama-server exited ({}).", status),
        );
        self.supervisor.on_server_exited(Instant::now())
    }
}

/// Translates live jobs until the channel closes (`None`: the translator
/// was stopped) or the server must be restarted or given up on.
fn serve(run: &mut ServerRun, rx: &Receiver<TranslationJob>) -> Option<SupervisorAction> {
    loop {
        let action = match rx.recv_timeout(IDLE_CHECK) {
            Ok(job) => run.live(job),
            Err(RecvTimeoutError::Timeout) => match run.server.exit_status() {
                Some(status) => run.server_exited(&status),
                None => SupervisorAction::Continue,
            },
            Err(RecvTimeoutError::Disconnected) => return None,
        };
        if action != SupervisorAction::Continue {
            return Some(action);
        }
    }
}

/// Sleeps `total`, in short steps; `false` if the worker was superseded.
fn sleep_while(total: Duration, is_current: &dyn Fn() -> bool) -> bool {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline {
        if !is_current() {
            return false;
        }
        thread::sleep(Duration::from_millis(200));
    }
    is_current()
}

/// No server any more: archive what still arrives, untranslated (it stays
/// owed in the ledger), until the translator is stopped or replaced.
fn drain_untranslated(app: &AppHandle, rx: &Receiver<TranslationJob>) {
    while let Ok(job) = rx.recv() {
        archive_chat(app, &job.chat);
    }
}

/// Translates, once per server start, the Japanese messages of this run
/// that the ledger still owes: the newest `translation_catch_up_limit` of
/// them, oldest first, read from the in-memory history. Jobs queued while
/// the server loaded are dropped first -- their messages are in the ledger.
/// Live messages arriving meanwhile go before the next catch-up item.
/// `None`: superseded; otherwise `Continue`, or the restart decision that
/// cut it short.
fn catch_up(
    run: &mut ServerRun,
    rx: &Receiver<TranslationJob>,
    is_current: &dyn Fn() -> bool,
) -> Option<SupervisorAction> {
    let app = run.app;
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
        return Some(SupervisorAction::Continue);
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
            return None;
        }
        emit_translator_state(app, "Catching Up", &format!("{}/{}", done + 1, total));
        while let Ok(job) = rx.try_recv() {
            let action = run.live(job);
            if action != SupervisorAction::Continue {
                return Some(action);
            }
        }
        let action = run.translate(TranslationJob::new(chat));
        if action != SupervisorAction::Continue {
            return Some(action);
        }
    }
    let still_owed = state.translation_ledger.lock().len();
    inject_system_message(
        app,
        SystemLogLevel::Info,
        "Translator",
        format!("Catch-up done ({} still untranslated).", still_owed),
    );
    Some(SupervisorAction::Continue)
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

enum JobResult {
    Done,
    /// Nothing was owed (translated already, or not of this run).
    Skipped,
    /// No translation; the message stays owed.
    Failed,
}

fn process_translation_job(job: TranslationJob, client: &Client, app: &AppHandle) -> JobResult {
    let chat = job.chat;
    let state = app.state::<crate::AppState>();

    // Translated already (a catch-up item that was also queued live) or not
    // a message of this run's ledger: nothing is owed.
    if !state.translation_ledger.lock().is_owed(chat.pid) {
        return JobResult::Skipped;
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
            return JobResult::Failed;
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
    JobResult::Done
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
