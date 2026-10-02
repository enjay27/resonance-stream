mod network;
mod raw_capture;

pub use self::network::*;
use self::raw_capture::RawCapture;
pub use self::raw_capture::{open_captures_folder, set_raw_capture};

use crate::{
    inject_system_message, inject_system_message_throttled, store_and_emit, TranslationJob,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::protocol::types::{AppState, SnifferState, SystemLogLevel};
use crossbeam_channel::Sender;
use resonance_core::capture::{ChatPipeline, PipelineAction};
use resonance_core::text::{contains_japanese, convert_to_romaji};
use resonance_core::workers::read_error_backoff;

// --- GLOBAL STATE ---
static LAST_TRAFFIC_TIME: AtomicU64 = AtomicU64::new(0);
static IS_SNIFFER_ACTIVE: AtomicBool = AtomicBool::new(false);

/// A started sniffer: dropping it stops the worker threads; `is_alive` says
/// whether the capture thread still runs. A sniffer that never started (no
/// firewall rule) or died (socket error) is not alive, so starting again is
/// not refused as "already active".
pub struct SnifferHandle {
    _stop: Sender<()>,
    alive: Arc<AtomicBool>,
}

impl SnifferHandle {
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

/// Clears the alive flag however the capture thread ends.
struct AliveGuard(Arc<AtomicBool>);

impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

// Helper to "Kick" or "Feed" the Watchdog
fn feed_watchdog() {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap();
    LAST_TRAFFIC_TIME.store(since_the_epoch.as_secs(), Ordering::Relaxed);
}

/// Records the state (for `get_service_states`) and emits it. The lock is
/// held across the emit so events leave in `seq` order.
pub fn emit_sniffer_state(app: &tauri::AppHandle, state: SnifferState, message: &str) {
    let Some(app_state) = app.try_state::<AppState>() else {
        return;
    };
    let mut states = app_state.service_states.lock();
    let payload = states.set_sniffer(state, message);
    let _ = app.emit("sniffer-state", payload);
}

#[tauri::command]
pub fn start_sniffer_command(app: AppHandle, state: State<'_, AppState>) {
    if !check_firewall_rule() {
        inject_system_message(
            &app,
            SystemLogLevel::Warning,
            "Sniffer",
            "Firewall rule missing. Triggering Setup Wizard.",
        );
        emit_sniffer_state(
            &app,
            SnifferState::Error,
            "방화벽 설정 필요 (Setup Required)",
        );
        let _ = app.emit("firewall-missing", ());
        return;
    }

    let mut tx_lock = state.sniffer_tx.lock();
    if tx_lock.as_ref().is_some_and(SnifferHandle::is_alive) {
        inject_system_message(
            &app,
            SystemLogLevel::Warning,
            "Sniffer",
            "Sniffer restart blocked: already active.",
        );
        emit_sniffer_state(&app, SnifferState::Pending, "Listening for game traffic...");
        IS_SNIFFER_ACTIVE.store(false, Ordering::Relaxed);
        return;
    }
    let tx = start_sniffer_worker(app.clone());
    *tx_lock = Some(tx);
}

pub fn start_sniffer_worker(app: AppHandle) -> SnifferHandle {
    // We use a blank channel just for its lifecycle dropping properties
    let (tx, rx) = crossbeam_channel::unbounded::<()>();
    let alive = Arc::new(AtomicBool::new(false));
    let handle = SnifferHandle {
        _stop: tx,
        alive: alive.clone(),
    };

    if !check_firewall_rule() {
        inject_system_message(
            &app,
            SystemLogLevel::Warning,
            "Sniffer",
            "Firewall rule missing. Triggering Setup Wizard.",
        );
        emit_sniffer_state(
            &app,
            SnifferState::Error,
            "방화벽 설정 필요 (Setup Required)",
        );

        // Tell the frontend to show the Setup Wizard!
        let _ = app.emit("firewall-missing", ());

        // Return immediately without spawning the network thread
        return handle;
    }

    let config = crate::config::current_config(&app);

    feed_watchdog();
    spawn_watchdog(app.clone(), rx.clone());

    // --- MAIN SNIFFER THREAD ---
    let app_handle = app.clone();
    let rx_main = rx.clone();

    alive.store(true, Ordering::SeqCst);
    thread::spawn(move || {
        let _alive = AliveGuard(alive);
        inject_system_message(
            &app_handle,
            SystemLogLevel::Success,
            "Sniffer",
            "Engine Active",
        );
        emit_sniffer_state(&app_handle, SnifferState::Starting, "Engine Active");
        IS_SNIFFER_ACTIVE.store(false, Ordering::Relaxed);

        // Abstracted Network Setup
        let socket = match initialize_network_socket(&app_handle, &config) {
            Some(s) => s,
            None => return,
        };

        inject_system_message(
            &app_handle,
            SystemLogLevel::Success,
            "Sniffer",
            "Raw Socket active. Listening for game traffic...",
        );
        emit_sniffer_state(
            &app_handle,
            SnifferState::Pending,
            "Listening for game traffic...",
        );

        let mut buf = [0u8; 65535];
        let state = app_handle.state::<AppState>();
        let mut pipeline = ChatPipeline::new();
        // History reloaded from disk: the server re-sending it after login is
        // not new chat.
        pipeline.remember(
            &state
                .chat_history
                .lock()
                .values()
                .cloned()
                .collect::<Vec<_>>(),
        );
        // Raw unparsed fields are only useful when reverse-engineering the protocol.
        pipeline.set_keep_unknown_fields(config.debug_mode);
        set_raw_capture(config.raw_capture);
        let mut raw_capture = RawCapture::default();
        let mut read_failures = 0u32;

        loop {
            if let Err(crossbeam_channel::TryRecvError::Disconnected) = rx_main.try_recv() {
                inject_system_message(
                    &app_handle,
                    SystemLogLevel::Info,
                    "Sniffer",
                    "Sniffer thread shutting down.",
                );
                break;
            }

            let uninit_buf = unsafe {
                std::slice::from_raw_parts_mut(
                    buf.as_mut_ptr() as *mut std::mem::MaybeUninit<u8>,
                    buf.len(),
                )
            };
            let n = match socket.recv(uninit_buf) {
                Ok(n) => {
                    read_failures = 0;
                    n
                }
                // The read timeout: nothing arrived, nothing is wrong.
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue
                }
                // A real failure: say so once per streak and back off, so a
                // socket that keeps failing does not spin a core.
                Err(e) => {
                    read_failures += 1;
                    if read_failures == 1 {
                        inject_system_message(
                            &app_handle,
                            SystemLogLevel::Warning,
                            "Sniffer",
                            format!("Socket read failed ({e}); retrying."),
                        );
                    }
                    thread::sleep(read_error_backoff(read_failures));
                    continue;
                }
            };

            // Debug capture of the untouched packet, before anything parses it.
            raw_capture.feed(&app_handle, &buf[..n]);

            // 1. Feed the Pure Pipeline. The raw socket sees every IP packet on
            // the interface, so nothing here may cost more than the parse: the
            // block list is only consulted for actual chat messages.
            let actions = pipeline.feed_network_packet(
                &buf[..n],
                |uid| state.blocked_users.lock().contains_key(&uid),
                || state.next_pid.fetch_add(1, Ordering::SeqCst),
                || {
                    feed_watchdog();

                    if !IS_SNIFFER_ACTIVE.load(Ordering::Relaxed) {
                        IS_SNIFFER_ACTIVE.store(true, Ordering::Relaxed);
                        emit_sniffer_state(
                            &app_handle,
                            SnifferState::Active,
                            "Listening for game traffic...",
                        );
                    }
                },
            );

            // 2. Dispatch Side Effects
            if !actions.is_empty() {
                dispatch_pipeline_actions(&app_handle, actions);
            }
        }
    });

    handle // Kept in AppState; dropping it stops the sniffer
}

// --- 2. WATCHDOG THREAD ---
fn spawn_watchdog(app: AppHandle, rx: crossbeam_channel::Receiver<()>) {
    thread::spawn(move || {
        loop {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                _ => {}
            }

            let last = LAST_TRAFFIC_TIME.load(Ordering::Relaxed);
            if last == 0 {
                continue;
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now.saturating_sub(last) > 15 {
                // If it was previously active, throw the error state
                inject_system_message_throttled(
                    &app,
                    SystemLogLevel::Warning,
                    "Sniffer",
                    "Watchdog: No game traffic for 15s.",
                );

                // Emitting "Error" changes the TitleBar badge to Red so the user can click it!
                emit_sniffer_state(
                    &app,
                    SnifferState::Error,
                    "게임 트래픽 감지 안됨 (클릭하여 어댑터 복구)",
                );
                IS_SNIFFER_ACTIVE.store(false, Ordering::Relaxed);

                // Kick the watchdog so we wait another 15s before checking again
                feed_watchdog();
            }
        }
    });
}

// --- 4. SIDE EFFECT DISPATCHER ---
fn dispatch_pipeline_actions(app: &AppHandle, actions: Vec<PipelineAction>) {
    let state = app.state::<AppState>();
    let use_translation = state.config.read().use_translation;

    for action in actions {
        match action {
            PipelineAction::UpdateBlockedMessage(chat) => {
                let mut history = state.chat_history.lock();
                if let Some(existing_msg) = history.get_mut(chat.pid) {
                    if !existing_msg.is_blocked {
                        existing_msg.is_blocked = true;
                        let _ = app.emit("chat-message-update", existing_msg.clone());
                    }
                }
            }
            PipelineAction::EmitNewMessage(mut chat) => {
                // Apply Romaji Swap
                if contains_japanese(&chat.nickname) {
                    let mut nick_cache = state.nickname_cache.lock();
                    chat.nickname_romaji = Some(
                        nick_cache
                            .entry(chat.nickname.clone())
                            .or_insert_with(|| convert_to_romaji(&chat.nickname))
                            .clone(),
                    );
                }

                // Dispatch Side Effects. A duplicate dropped here is neither
                // translated nor archived.
                if !store_and_emit(app, chat.clone()) {
                    continue;
                }

                // Every Japanese message is owed a translation, even with the
                // translator off or still starting: its next start catches up.
                if contains_japanese(&chat.message) {
                    state.translation_ledger.lock().record(chat.pid);
                }

                // Translated messages are archived by the translator with their
                // translation; anything else is archived as it is.
                let translator = state.translator_tx.lock();
                match translator.as_ref() {
                    Some(tx) if use_translation && contains_japanese(&chat.message) => {
                        let _ = tx.send(TranslationJob::new(chat));
                    }
                    _ => crate::services::translator::archive_chat(app, &chat),
                }
            }
        }
    }
}

#[tauri::command(async)]
pub fn block_user_command(
    uid: u64,
    nickname: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) {
    // 1. Add to In-Memory AppState
    state.blocked_users.lock().insert(uid, nickname.clone());

    // 2. Add to Disk Config
    crate::config::modify_config(&app, &state, |config| {
        config.blocked_users.insert(uid, nickname);
    });

    // 3. Retroactively scrub existing messages in the UI
    let mut history = state.chat_history.lock();
    for msg in history.values_mut() {
        if msg.uid == uid && !msg.is_blocked {
            msg.is_blocked = true;
            let _ = app.emit("chat-message-update", msg.clone());
        }
    }
}

#[tauri::command(async)]
pub fn unblock_user_command(uid: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    // 1. Remove from In-Memory AppState
    state.blocked_users.lock().remove(&uid);

    // 2. Remove from Disk Config
    crate::config::modify_config(&app, &state, |config| {
        config.blocked_users.remove(&uid);
    });

    // 3. Retroactively un-scrub existing messages in the UI
    let mut history = state.chat_history.lock();
    for msg in history.values_mut() {
        if msg.uid == uid && msg.is_blocked {
            msg.is_blocked = false;
            let _ = app.emit("chat-message-update", msg.clone());
        }
    }
}

#[tauri::command]
pub fn restart_sniffer_command(app: tauri::AppHandle) {
    // On its own thread: the pause below would otherwise freeze the window
    // (synchronous commands run on the main thread).
    thread::spawn(move || {
        let state = app.state::<AppState>();

        // 1. Drop the sender to safely terminate the old sniffer thread
        *state.sniffer_tx.lock() = None;

        // 2. Wait a moment for the OS to release the socket binding
        thread::sleep(Duration::from_millis(500));

        // 3. Start a fresh sniffer!
        let tx = start_sniffer_worker(app.clone());
        *state.sniffer_tx.lock() = Some(tx);
    });
}
