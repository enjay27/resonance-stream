mod stream_traacker;
mod message_processor;
mod pipeline;
mod network;

pub use self::network::*;
pub use self::pipeline::*;

use crate::{inject_system_message, store_and_emit, NetworkInterface, SnifferStatePayload, TranslationJob};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, thread};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::protocol::types::{
    AppState, SystemLogLevel
};
use crate::services::sniffer::pipeline::PipelineAction;
use crate::services::translator::core::contains_japanese;
use crate::services::translator::processor::convert_to_romaji;
use crossbeam_channel::Sender;
use local_ip_address::list_afinet_netifas;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::Ipv4Addr;
use std::os::windows::process::CommandExt;
use std::process::Command;
use crate::services::sniffer::network::initialize_network_socket;

// --- GLOBAL STATE ---
static LAST_TRAFFIC_TIME: AtomicU64 = AtomicU64::new(0);

// Helper to "Kick" or "Feed" the Watchdog
fn feed_watchdog() {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap();
    LAST_TRAFFIC_TIME.store(since_the_epoch.as_secs(), Ordering::Relaxed);
}

pub fn emit_sniffer_state(app: &tauri::AppHandle, state: &str, message: &str) {
    let _ = app.emit("sniffer-state", SnifferStatePayload {
        state: state.to_string(),
        message: message.to_string(),
    });
}

#[tauri::command]
pub fn start_sniffer_command(window: tauri::Window, app: AppHandle, state: State<'_, AppState>) {
    let mut tx_lock = state.sniffer_tx.lock().unwrap();
    if tx_lock.is_some() {
        inject_system_message(&app, SystemLogLevel::Warning, "Sniffer", "Sniffer restart blocked: already active.");
        emit_sniffer_state(&app, "Active", "Listening for game traffic...");
        return;
    }
    let tx = start_sniffer_worker(app.clone());
    *tx_lock = Some(tx);
}

pub fn start_sniffer_worker(app: AppHandle) -> Sender<()> {
    // We use a blank channel just for its lifecycle dropping properties
    let (tx, rx) = crossbeam_channel::unbounded::<()>();
    let config = crate::config::load_config(app.clone());

    feed_watchdog();
    spawn_watchdog(app.clone(), rx.clone());

    // --- MAIN SNIFFER THREAD ---
    let app_handle = app.clone();
    let rx_main = rx.clone();

    thread::spawn(move || {
        inject_system_message(&app_handle, SystemLogLevel::Success, "Sniffer", "Engine Active");
        emit_sniffer_state(&app_handle, "Starting", "Engine Active");

        // Abstracted Network Setup
        let socket = match initialize_network_socket(&app_handle, &config) {
            Some(s) => s,
            None => return,
        };

        inject_system_message(&app_handle, SystemLogLevel::Success, "Sniffer", "Raw Socket active. Listening for game traffic...");
        emit_sniffer_state(&app_handle, "Active", "Listening for game traffic...");

        let mut buf = [0u8; 65535];
        let mut pipeline = pipeline::ChatPipeline::new();

        loop {
            if let Err(crossbeam_channel::TryRecvError::Disconnected) = rx_main.try_recv() {
                inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", "Sniffer thread shutting down.");
                break;
            }

            let uninit_buf = unsafe { std::mem::transmute::<&mut [u8], &mut [std::mem::MaybeUninit<u8>]>(buf.as_mut_slice()) };
            let n = match socket.recv(uninit_buf) {
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => continue,
            };

            feed_watchdog();

            let state = app_handle.state::<AppState>();
            let blocked_users = state.blocked_users.lock().unwrap().clone();

            // 1. Feed the Pure Pipeline
            let actions = pipeline.feed_network_packet(&buf[..n], &blocked_users, || {
                state.next_pid.fetch_add(1, Ordering::SeqCst)
            });

            // 2. Dispatch Side Effects
            dispatch_pipeline_actions(&app_handle, actions);
        }
    });

    tx // Return the Sender to AppState!
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
            if last == 0 { continue; }

            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            if now.saturating_sub(last) > 15 {
                inject_system_message(&app, SystemLogLevel::Warning, "Sniffer", "Watchdog: No game traffic for 15s.");
                let _ = app.emit("sniffer-status", "warning");
                feed_watchdog();
            }
        }
    });
}

// --- 4. SIDE EFFECT DISPATCHER ---
fn dispatch_pipeline_actions(app: &AppHandle, actions: Vec<PipelineAction>) {
    let state = app.state::<AppState>();
    let config = crate::config::load_config(app.clone());

    for action in actions {
        match action {
            PipelineAction::UpdateBlockedMessage(chat) => {
                let mut history = state.chat_history.lock().unwrap();
                if let Some(existing_msg) = history.get_mut(&chat.pid) {
                    if !existing_msg.is_blocked {
                        existing_msg.is_blocked = true;
                        let _ = app.emit("chat-message-update", existing_msg.clone());
                    }
                }
            },
            PipelineAction::EmitNewMessage(mut chat) => {
                // Apply Romaji Swap
                if contains_japanese(&chat.nickname) {
                    let mut nick_cache = state.nickname_cache.lock().unwrap();
                    chat.nickname_romaji = Some(nick_cache.entry(chat.nickname.clone())
                        .or_insert_with(|| convert_to_romaji(&chat.nickname)).clone());
                }

                // Dispatch Side Effects
                store_and_emit(app, chat.clone());

                if config.use_translation && contains_japanese(&chat.message) {
                    if let Some(tx) = state.translator_tx.lock().unwrap().as_ref() {
                        let _ = tx.send(TranslationJob { chat: chat.clone() });
                    }
                } else if config.archive_chat {
                    if let Some(df_tx) = state.data_factory_tx.lock().unwrap().as_ref() {
                        let _ = df_tx.send(crate::io::DataFactoryJob {
                            pid: chat.pid,
                            original: chat.message.clone(),
                            translated: None,
                        });
                    }
                }
            }
        }
    }
}

#[tauri::command]
pub fn block_user_command(uid: u64, nickname: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    // 1. Add to In-Memory AppState
    state.blocked_users.lock().unwrap().insert(uid, nickname.clone());

    // 2. Add to Disk Config
    let mut config = crate::config::load_config(app.clone());
    config.blocked_users.insert(uid, nickname);

    // Pass app and state exactly as your config.rs requires
    crate::config::save_config(app.clone(), state.clone(), config);

    // 3. Retroactively scrub existing messages in the UI
    let mut history = state.chat_history.lock().unwrap();
    for (_, msg) in history.iter_mut() {
        if msg.uid == uid && !msg.is_blocked {
            msg.is_blocked = true;
            let _ = app.emit("chat-message-update", msg.clone());
        }
    }
}

#[tauri::command]
pub fn unblock_user_command(uid: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    // 1. Remove from In-Memory AppState
    state.blocked_users.lock().unwrap().remove(&uid);

    // 2. Remove from Disk Config
    let mut config = crate::config::load_config(app.clone());
    config.blocked_users.remove(&uid);

    crate::config::save_config(app.clone(), state.clone(), config);

    // 3. Retroactively un-scrub existing messages in the UI
    let mut history = state.chat_history.lock().unwrap();
    for (_, msg) in history.iter_mut() {
        if msg.uid == uid && msg.is_blocked {
            msg.is_blocked = false;
            let _ = app.emit("chat-message-update", msg.clone());
        }
    }
}