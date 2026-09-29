//! Backend -> UI events that are also kept as history: system log lines
//! ("system-event") and game chat ("packet-event").

use crate::{load_config, AppState, ChatMessage, SystemLogLevel, SystemMessage};
use lazy_static::lazy_static;
use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
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
            let mut sys_hist = state.system_history.lock().unwrap();
            if sys_hist.len() >= 200 {
                sys_hist.pop_front();
            }
            sys_hist.push_back(system_message.clone());
        }

        let _ = app.emit("system-event", &system_message);
    }
}

pub fn store_and_emit(app: &tauri::AppHandle, mut packet: ChatMessage) {
    let fingerprint = generate_message_fingerprint(&packet);
    let now = Instant::now();

    {
        let mut cache = CHAT_DEDUPE_CACHE.lock().unwrap();

        // 1. Prune old messages from the sliding window (e.g., older than 2 seconds)
        while let Some(&(_, time)) = cache.front() {
            if now.duration_since(time) > Duration::from_secs(2) {
                cache.pop_front();
            } else {
                break; // VecDeque is ordered by time, so we can stop here
            }
        }

        // 2. Check if this exact message was already processed
        if cache.iter().any(|(hash, _)| *hash == fingerprint) {
            // Silently drop the duplicate packet from the second client
            return;
        }

        // 3. Not a duplicate, add it to the cache
        cache.push_back((fingerprint, now));
    }

    if let Some(state) = app.try_state::<AppState>() {
        // Auto-populate from Backend Cache
        {
            let cache = state.nickname_cache.lock().unwrap();
            if let Some(romaji) = cache.get(&packet.nickname) {
                packet.nickname_romaji = Some(romaji.clone());
            }
        }

        // Store in HOT Storage (IndexMap)
        {
            let config = load_config(app.clone());

            let mut history = state.chat_history.lock().unwrap();
            while history.len() >= config.chat_limit && !history.is_empty() {
                history.shift_remove_index(0);
            }
            history.insert(packet.pid, packet.clone());
        }

        // Emit "packet-event" for Game Chat
        let _ = app.emit("packet-event", &packet);
    }
}

// Helper to generate a unique fingerprint for the chat message
fn generate_message_fingerprint(packet: &ChatMessage) -> u64 {
    let mut hasher = DefaultHasher::new();

    // If your server provides a truly unique sequence_id for every message,
    // you only need to hash that. Otherwise, hash the combination of sender, text, and time:
    packet.uid.hash(&mut hasher);
    packet.message.hash(&mut hasher);
    packet.timestamp.hash(&mut hasher);

    hasher.finish()
}
