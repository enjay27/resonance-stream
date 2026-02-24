use crate::packet_buffer::PacketBuffer;
use crate::{inject_system_message, store_and_emit};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State, Window};
use windivert::prelude::*;

use crate::protocol::types::{
    ChatMessage, AppState, SystemLogLevel, MessageRequest,
    SystemMessage, LobbyRecruitment, ProfileAsset
};
use crate::protocol::parser::{self, Port5003Event, SplitPayload};

// --- DATA STRUCTURES ---
// 1. Standard Chat: Focuses on player communication

lazy_static! {
    // Tracks already seen fields to prevent log flooding
    static ref DISCOVERED_FIELDS: Mutex<HashSet<u32>> = Mutex::new(HashSet::from([
        1, 2, 3, 7, 10, 11, 13, 15, 18, 20, 25, 34, 35
    ]));

    static ref DISCOVERED_FIELDS_5003: Mutex<HashSet<u32>> = Mutex::new(HashSet::from([
        0, 1, 2, 3, 4, 5, 6, 7, 12, 16, 17, 18, 21, 22, 23, 24, 25, 26, 29, 30, 31
    ]));

    // Stores (Hash, Last_Seen_Instant) per ID/UID
    static ref EMISSION_CACHE: Mutex<HashMap<String, (u64, Instant)>> = Mutex::new(HashMap::new());
}

// --- GLOBAL STATE ---
static IS_SNIFFER_RUNNING: AtomicBool = AtomicBool::new(false);

// Watchdog Timer (Last time we saw a packet)
static LAST_TRAFFIC_TIME: AtomicU64 = AtomicU64::new(0);

// Generation Counter: Allows us to "soft kill" old threads safely
static SNIFFER_GENERATION: AtomicU64 = AtomicU64::new(0);

// Helper to "Kick" or "Feed" the Watchdog
fn feed_watchdog() {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap();
    LAST_TRAFFIC_TIME.store(since_the_epoch.as_secs(), Ordering::Relaxed);
}

#[tauri::command]
pub fn start_sniffer_command(window: tauri::Window, app: AppHandle, state: State<'_, AppState>) {
    start_sniffer(window, app, state);
}

fn start_sniffer(window: Window, app: AppHandle, state: State<'_, AppState>) {
    // 1. Check if we are "officially" running
    if IS_SNIFFER_RUNNING.load(Ordering::SeqCst) {
        inject_system_message(&app, SystemLogLevel::Warning, "Sniffer", "Sniffer restart blocked: already active.");
        return;
    }

    IS_SNIFFER_RUNNING.store(true, Ordering::SeqCst);
    state.next_pid.store(1, Ordering::SeqCst);

    // 2. Increment Generation (This kills the old thread logically)
    let config = crate::config::load_config(app.clone()); //
    let my_generation = SNIFFER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    if config.is_debug {
        use local_ip_address::list_afinet_netifas;

        if let Ok(network_interfaces) = list_afinet_netifas() {
            for (name, ip) in network_interfaces {
                inject_system_message(&app, SystemLogLevel::Info, "Sniffer", format!("Active Interface: {} ({:?})", name, ip));
            }
        }
    }

    // Reset watchdog on start
    feed_watchdog();

    // --- WATCHDOG THREAD (Red Dot Logic) ---
    let app_handle_watchdog = app.clone();
    thread::spawn(move || {
        let mut last_cleanup = Instant::now();
        loop {
            thread::sleep(std::time::Duration::from_secs(5));

            // [CHECK] If I am an old watchdog for a dead sniffer, I must retire.
            if SNIFFER_GENERATION.load(Ordering::Relaxed) != my_generation {
                break;
            }

            let last = LAST_TRAFFIC_TIME.load(Ordering::Relaxed);
            if last == 0 { continue; }

            let start = SystemTime::now();
            let now = start.duration_since(UNIX_EPOCH).unwrap().as_secs();

            // TRIGGER: No packets for 15 seconds
            if now.saturating_sub(last) > 15 {
                inject_system_message(&app_handle_watchdog, SystemLogLevel::Warning, "Sniffer", "Watchdog: No game traffic for 15s.");

                // Emit event to Frontend (Red Dot)
                let _ = app_handle_watchdog.emit("sniffer-status", "warning");

                // Feed it to prevent spamming the warning every 5 seconds
                feed_watchdog();
            }

            if last_cleanup.elapsed().as_secs() >= 60 {
                let mut cache = EMISSION_CACHE.lock().unwrap();
                let now = Instant::now();

                // Retain only fresh items, emit removal for expired ones
                cache.retain(|key, (_, last_seen)| {
                    if now.duration_since(*last_seen) > Duration::from_secs(300) {
                        let _ = app_handle_watchdog.emit("remove-entity", key);
                        false
                    } else { true }
                });
                last_cleanup = Instant::now();
            }
        }
    });

    // --- MAIN SNIFFER THREAD ---
    let app_handle = app.clone();
    thread::spawn(move || {
        inject_system_message(&app_handle, SystemLogLevel::Success, "Sniffer", format!("Engine Active (Gen {})", my_generation));

        let filter = "tcp.PayloadLength > 0 and (tcp.SrcPort == 5003 or tcp.DstPort == 5003)";
        let flags = WinDivertFlags::new().set_sniff();

        let wd = match WinDivert::network(filter, 0, flags) {
            Ok(w) => w,
            Err(e) => {
                // [NEW] Map common WinDivert error codes for the user
                let err_str = format!("{:?}", e);
                let diagnostic = if err_str.contains("Code 5") {
                    "ACCESS_DENIED: Please Run as Administrator."
                } else if err_str.contains("Code 577") {
                    "INVALID_IMAGE_HASH: Driver signature blocked. Check Secure Boot or Windows Update."
                } else {
                    "DRIVER_INIT_FAILED: Ensure WinDivert64.sys is in the app folder."
                };

                inject_system_message(&app_handle, SystemLogLevel::Error, "Sniffer", format!("FATAL: {}", diagnostic));
                IS_SNIFFER_RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };

        let mut streams: HashMap<[u8; 6], PacketBuffer> = HashMap::new();
        let mut buffer = [0u8; 65535];

        loop {
            // [CRITICAL] COOPERATIVE SHUTDOWN
            if SNIFFER_GENERATION.load(Ordering::Relaxed) != my_generation {
                inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", format!("Sniffer Gen {} Shutdown signal received. Exiting.", my_generation));
                break; // This drops 'wd', closing the handle cleanly.
            }

            if let Ok(packet) = wd.recv(Some(&mut buffer)) {
                if config.is_debug && LAST_TRAFFIC_TIME.load(Ordering::Relaxed) == 0 {
                    inject_system_message(&app_handle, SystemLogLevel::Success, "Sniffer", "First Packet Captured! Network link established.");
                }
                feed_watchdog();

                let raw_data = packet.data;

                if let Some(stream_key) = parser::extract_stream_key(&*raw_data) {
                    let port = u16::from_be_bytes([stream_key[4], stream_key[5]]);

                    if let Some(payload) = parser::extract_tcp_payload(&*raw_data) {
                        if let Some(game_data) = parser::strip_application_header(payload, port) {
                            let p_buf = streams.entry(stream_key).or_insert_with(PacketBuffer::new);
                            p_buf.add(game_data);

                            while let Some(full_packet) = p_buf.next() {
                                if port == 5003 {
                                    // Try Chat first, then Party, then Broadcasts
                                    parse_and_emit_5003(&full_packet, &app_handle);
                                }
                            }
                        }
                    }
                }
            }
        }
        inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", "Old Sniffer Thread Terminated.");
    });
}

// --- STAGE 3: EMIT ---
// Filters out duplicates and dispatches the final events to Tauri.
pub fn parse_and_emit_5003(data: &[u8], app: &AppHandle) {
    // If it's a server packet, this safely returns without spamming logs
    let raw_payload = match crate::protocol::parser::stage1_split(data) {
        Some(p) => p,
        None => return,
    };

    let events = crate::protocol::parser::stage2_process(raw_payload);

    for event in events {
        if should_emit(&event) {
            match event {
                Port5003Event::Chat(c) => store_and_emit(app, c),
                Port5003Event::Recruit(l) => { let _ = app.emit("lobby-update", l); },
                Port5003Event::Asset(a) => { let _ = app.emit("profile-asset-update", a); },
            }
        }
    }
}

// --- DEDUPLICATION LOGIC ---
fn should_emit(event: &Port5003Event) -> bool {
    let mut cache = EMISSION_CACHE.lock().unwrap();
    let now = Instant::now();

    let (key, content_to_hash) = match event {
        Port5003Event::Recruit(l) => (format!("recruit_{}", l.recruit_id), &l.description),
        Port5003Event::Asset(a) => (format!("asset_{}", a.uid), &a.snapshot_url),
        Port5003Event::Chat(_) => return true, // Chat bypasses dedupe
    };

    let mut hasher = DefaultHasher::new();
    content_to_hash.hash(&mut hasher);
    let new_hash = hasher.finish();

    if let Some((old_hash, last_seen)) = cache.get_mut(&key) {
        *last_seen = now;
        if *old_hash == new_hash { return false; }
        *old_hash = new_hash;
        return true;
    }

    cache.insert(key, (new_hash, now));
    true
}

/// Logs a warning with full packet data when a new protocol field is found
fn log_missing_field_5003(app: &AppHandle, field_num: u32, wire_type: u8, data: &[u8]) {
    let mut discovered = DISCOVERED_FIELDS_5003.lock().unwrap();
    if discovered.insert(field_num) {
        inject_system_message(
            app,
            SystemLogLevel::Warning,
            "Discovery-5003",
            format!("New Chat/Party Field #{} (Wire {}). Packet: {:?}", field_num, wire_type, data)
        );
    }
}

fn split_port_5003_fields(data: &[u8]) -> HashMap<u32, Vec<u8>> {
    let mut fields = HashMap::new();
    if data.len() < 2 || data[0] != 0x0A { return fields; }

    let (total_len, header_read) = crate::protocol::parser::read_varint(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    while i < safe_end {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = (tag >> 3) as u32;
        i += 1;

        let start = i;
        let consumed = crate::protocol::parser::skip_field(wire_type, &data[i..safe_end]);
        i += consumed;
        let end = i.min(safe_end);

        if let Some(payload) = data.get(start..end) {
            // Use entry to handle repeated fields if necessary
            fields.insert(field_num, payload.to_vec());
        }
    }
    fields
}