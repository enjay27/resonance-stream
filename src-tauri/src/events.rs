//! Backend -> UI events that are also kept as history: system log lines
//! ("system-event") and game chat ("packet-event").

use crate::{AppState, ChatMessage, SystemLogLevel, SystemMessage};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

lazy_static! {
    // Stores: (Message Fingerprint, Arrival Time)
    static ref CHAT_DEDUPE_CACHE: Mutex<VecDeque<(u64, Instant)>> = Mutex::new(VecDeque::new());
}

pub fn inject_system_message<S: Into<String>>(
    app: &tauri::AppHandle,
    level: SystemLogLevel,
    source: &str,
    message: S,
) {
    let msg = message.into();

    if let Some(state) = app.try_state::<AppState>() {
        let current_pid = state.next_pid.fetch_add(1, Ordering::SeqCst);

        // Map the Enum to the string expected by the frontend SystemMessage struct
        let log_message = format!("[{}] {}", source, msg);
        let level_str = match level {
            SystemLogLevel::Info => {
                log::info!("{}", log_message);
                "info"
            }
            SystemLogLevel::Warning => {
                log::warn!("{}", log_message);
                "warn"
            }
            SystemLogLevel::Error => {
                log::error!("{}", log_message);
                "error"
            }
            SystemLogLevel::Success => {
                log::info!("{}", log_message);
                "success"
            }
            SystemLogLevel::Debug => {
                log::debug!("{}", log_message);
                "debug"
            }
            SystemLogLevel::Trace => {
                log::trace!("{}", log_message);
                "trace"
            }
        };

        let system_message = SystemMessage {
            pid: current_pid,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            level: level_str.to_string(),
            source: source.to_string(),
            message: msg,
        };

        // Store in specialized system storage
        {
            let mut sys_hist = state.system_history.lock();
            if sys_hist.len() >= 200 {
                sys_hist.pop_front();
            }
            sys_hist.push_back(system_message.clone());
        }

        let _ = app.emit("system-event", &system_message);
    }
}

/// Stores and emits a chat message. Returns `false` when it was dropped as a
/// duplicate, so the caller does not translate or archive it either.
pub fn store_and_emit(app: &tauri::AppHandle, mut packet: ChatMessage) -> bool {
    let fingerprint = resonance_core::capture::fingerprint(&packet);
    let now = Instant::now();

    if let Some(fingerprint) = fingerprint {
        let mut cache = CHAT_DEDUPE_CACHE.lock();

        // Prune the sliding window (older than 2 seconds); the deque is ordered by time.
        while let Some(&(_, time)) = cache.front() {
            if now.duration_since(time) > Duration::from_secs(2) {
                cache.pop_front();
            } else {
                break;
            }
        }

        // The same message from a second client: drop it.
        if cache.iter().any(|(hash, _)| *hash == fingerprint) {
            return false;
        }
        cache.push_back((fingerprint, now));
    }

    if let Some(state) = app.try_state::<AppState>() {
        // Auto-populate from Backend Cache
        {
            let cache = state.nickname_cache.lock();
            if let Some(romaji) = cache.get(&packet.nickname) {
                packet.nickname_romaji = Some(romaji.clone());
            }
        }

        // Store in HOT Storage (bounded by the largest tab limit, oldest dropped first)
        state.chat_history.lock().push(packet.clone());

        // Emit "packet-event" for Game Chat
        let _ = app.emit("packet-event", &packet);
    }
    true
}
