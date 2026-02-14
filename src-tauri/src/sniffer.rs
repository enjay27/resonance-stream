use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use indexmap::IndexMap;
use windivert::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, Window};
use crate::{inject_system_message, store_and_emit};
use crate::packet_buffer::PacketBuffer;

// --- DATA STRUCTURES ---
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatPacket {
    pub pid: u64,
    pub channel: String,
    pub entity_id: u64,
    pub uid: u64,
    pub nickname: String,
    pub class_id: u64,
    pub status_flag: u64,
    pub level: u64,
    pub timestamp: u64,
    pub message: String,
    #[serde(default)]
    pub translated: Option<String>,
    #[serde(default)]
    pub nickname_romaji: Option<String>,
}

pub struct AppState {
    pub tx: Mutex<Option<tauri_plugin_shell::process::CommandChild>>,
    pub chat_history: Mutex<IndexMap<u64, ChatPacket>>,
    pub system_history: Mutex<VecDeque<ChatPacket>>,
    pub next_pid: AtomicU64,
    pub nickname_cache: Mutex<HashMap<String, String>>,
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
        inject_system_message(&app, "System: Sniffer already active.");
        return;
    }

    IS_SNIFFER_RUNNING.store(true, Ordering::SeqCst);
    state.next_pid.store(1, Ordering::SeqCst);

    // 2. Increment Generation (This kills the old thread logically)
    let my_generation = SNIFFER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    // Reset watchdog on start
    feed_watchdog();

    // --- WATCHDOG THREAD (Red Dot Logic) ---
    let app_handle_watchdog = app.clone();
    thread::spawn(move || {
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
                inject_system_message(&app_handle_watchdog, "[Warning] No game traffic detected for 15s.");

                // Emit event to Frontend (Red Dot)
                let _ = app_handle_watchdog.emit("sniffer-status", "warning");

                // Feed it to prevent spamming the warning every 5 seconds
                feed_watchdog();
            }
        }
    });

    // --- MAIN SNIFFER THREAD ---
    let app_handle = app.clone();
    thread::spawn(move || {
        inject_system_message(&app_handle, format!("Eye of Star Resonance: Active (Gen {})", my_generation));

        let filter = "tcp.PayloadLength > 0 and (tcp.SrcPort == 5003 or tcp.DstPort == 5003)";
        let flags = WinDivertFlags::new().set_sniff();

        let wd = match WinDivert::network(filter, 0, flags) {
            Ok(w) => w,
            Err(e) => {
                inject_system_message(&app_handle, format!("[Sniffer] FATAL ERROR: {:?}", e));
                IS_SNIFFER_RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };

        let mut streams: HashMap<[u8; 6], PacketBuffer> = HashMap::new();
        let mut buffer = [0u8; 65535];

        loop {
            // [CRITICAL] COOPERATIVE SHUTDOWN
            // If the global generation has changed, I am obsolete.
            if SNIFFER_GENERATION.load(Ordering::Relaxed) != my_generation {
                inject_system_message(&app_handle, format!("[Sniffer Gen {}] Shutdown signal received. Exiting.", my_generation));
                break; // This drops 'wd', closing the handle cleanly.
            }

            if let Ok(packet) = wd.recv(Some(&mut buffer)) {
                // Feed the Watchdog because we saw traffic
                feed_watchdog();

                let raw_data = packet.data;

                // 1. Get the Stream Key
                if let Some(stream_key) = extract_stream_key(&*raw_data) {

                    // 2. Extract Payload
                    if let Some(payload) = extract_tcp_payload(&*raw_data) {

                        // Skip empty ACKs
                        // 1. Minimum Header Validation (4 bytes Length + 2 bytes OpCode)
                        if payload.len() < 6 { continue; }

                        // Extract the Type/Sequence field
                        let type_field = (payload[4] as u16) << 8 | (payload[5] as u16);

                        // BLACKLIST: Ignore only known non-chat high-traffic packets
                        // 0x0004 = Heartbeat
                        // 0x8002 = Massive Character Sync
                        if type_field == 0x0004 || type_field == 0x8002 {
                            if type_field == 0x8002 { streams.remove(&stream_key); }
                            continue;
                        }

                        // Allow all other types (0x0001, 0x0002, 0x0003...)
                        let p_buf = streams.entry(stream_key).or_insert_with(PacketBuffer::new);
                        p_buf.add(payload);

                        while let Some(full_packet) = p_buf.next() {
                            if let Some(chat) = parse_star_resonance(&full_packet) {
                                store_and_emit(&app_handle, chat);
                            }
                        }
                    }
                }
            }
        }

        inject_system_message(&app_handle, "Old Sniffer Thread Terminated.");
    });
}

// ==========================================
// PARSING & UTILITIES (Keep your existing functions below)
// ==========================================

pub fn parse_star_resonance(data: &[u8]) -> Option<ChatPacket> {
    if data.len() < 3 || data[0] != 0x0A { return None; }

    let mut chat = ChatPacket::default();
    let (total_len, header_read) = read_varint(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    while i < safe_end {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // Standard Channel
                let (val, read) = read_varint(&data[i..safe_end]);
                chat.channel = match val {
                    2 => "LOCAL".into(), 3 => "PARTY".into(), 4 => "GUILD".into(), _ => "WORLD".into(),
                };
                i += read;
            }
            2 => { // Standard Chat (User Profile)
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);
                if let Some(sub_data) = data.get(i..block_end) {
                    parse_user_container(sub_data, &mut chat);
                }
                i = block_end;
            }
            4 => { // Broadcast/Recruitment Block
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);

                if let Some(sub_data) = data.get(i..block_end) {
                    // Message text is always Tag 0x1A (Field 3)
                    if let Some(msg) = find_string_by_tag(sub_data, 0x1A) {
                        chat.message = msg;

                        // Extract specific channel (Tag 0x10 / Field 2)
                        // Your packet showed value 3 (PARTY)
                        if let Some(chan_id) = find_int_by_tag(sub_data, 0x10) {
                            chat.channel = match chan_id {
                                3 => "PARTY".into(), 4 => "GUILD".into(), _ => chat.channel
                            };
                        }

                        // Label as recruitment if no user profile was found
                        // Recruit will be released in the future release
                        // if chat.uid == 0 {
                        //     chat.nickname = format!("{}_RECRUIT", chat.channel);
                        //     chat.uid = 999; // Placeholder to pass the final check
                        // }
                    }
                }
                i = block_end;
            }
            _ => i += skip_field(wire_type, &data[i..safe_end]),
        }
    }

    if !chat.message.is_empty() && chat.uid > 0 { Some(chat) } else { None }
}

fn find_int_by_tag(data: &[u8], target_tag: u8) -> Option<u64> {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        if tag == target_tag {
            let (val, _) = read_varint(&data[i+1..]);
            return Some(val);
        }
        let wire_type = tag & 0x07;
        i += 1 + skip_field(wire_type, &data[i+1..]);
    }
    None
}

fn parse_user_container(data: &[u8], chat: &mut ChatPacket) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // Entity ID (Session ID)
                let (val, read) = read_varint(&data[i..]);
                chat.entity_id = val;
                i += read;
            }
            2 => { // Profile Block
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    parse_profile_block(sub_data, chat);
                }
                i = block_end;
            }
            3 => { // Timestamp
                let (val, read) = read_varint(&data[i..]);
                chat.timestamp = val;
                i += read;
            }
            4 => { // Message Block
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    if let Some(msg) = find_string_by_tag(sub_data, 0x1A) {
                        chat.message = msg;
                    }
                }
                i = block_end;
            }
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
}

fn parse_profile_block(data: &[u8], chat: &mut ChatPacket) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // Permanent UID
                let (val, read) = read_varint(&data[i..]);
                chat.uid = val;
                i += read;
            }
            2 => { // Nickname
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    chat.nickname = String::from_utf8_lossy(sub_data).into_owned();
                }
                i = block_end;
            }
            3 => { // Class ID
                let (val, read) = read_varint(&data[i..]);
                chat.class_id = val;
                i += read;
            }
            4 => { // Status Flag
                let (val, read) = read_varint(&data[i..]);
                chat.status_flag = val;
                i += read;
            }
            5 => { // Level
                let (val, read) = read_varint(&data[i..]);
                chat.level = val;
                i += read;
            }
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
}

fn find_string_by_tag(data: &[u8], target_tag: u8) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        if tag == target_tag {
            let (len, read) = read_varint(&data[i+1..]);
            let start = i + 1 + read;
            let end = (start + len as usize).min(data.len());
            if start < end {
                return Some(String::from_utf8_lossy(&data[start..end]).into_owned());
            }
        }
        let wire_type = tag & 0x07;
        i += 1 + skip_field(wire_type, &data[i+1..]);
    }
    None
}

fn read_varint(data: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    let mut pos = 0;
    while pos < data.len() {
        let byte = data[pos];
        if shift >= 64 {
            return (value, pos); // Stop if we hit 64 bits to prevent panic
        }
        value |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if (byte & 0x80) == 0 { break; }
        shift += 7;
    }
    (value, pos)
}

fn skip_field(wire_type: u8, data: &[u8]) -> usize {
    match wire_type {
        0 => read_varint(data).1,
        1 => 8,
        2 => {
            let (len, read) = read_varint(data);
            read + len as usize
        }
        5 => 4,
        _ => 1,
    }
}

fn extract_stream_key(data: &[u8]) -> Option<[u8; 6]> {
    if data.len() < 20 || (data[0] >> 4) != 4 || data[9] != 6 { return None; } // Must be IPv4 + TCP
    let ihl = (data[0] & 0x0F) as usize * 4;
    if data.len() < ihl + 20 { return None; }

    let mut key = [0u8; 6];
    key[0..4].copy_from_slice(&data[12..16]); // Source IP
    key[4..6].copy_from_slice(&data[ihl..ihl + 2]); // Source Port
    Some(key)
}

fn extract_tcp_payload(data: &[u8]) -> Option<&[u8]> {
    // Basic IPv4 check
    if data.len() < 20 || (data[0] >> 4) != 4 || data[9] != 6 { return None; }

    // IP Header Length (usually 20 bytes, but can be more)
    let ip_header_len = (data[0] & 0x0F) as usize * 4;
    if data.len() < ip_header_len + 20 { return None; }

    // TCP Header Length (Offset is at byte 12 of the TCP header)
    let tcp_header_len = ((data[ip_header_len + 12] >> 4) as usize) * 4;

    // The actual game data starts after both headers
    let payload_offset = ip_header_len + tcp_header_len;

    if payload_offset <= data.len() {
        Some(&data[payload_offset..])
    } else {
        None
    }
}

#[tauri::command]
pub fn get_chat_history(state: tauri::State<AppState>) -> Vec<ChatPacket> {
    // Returns ONLY Game Chat
    let history = state.chat_history.lock().unwrap();
    history.values().cloned().collect()
}

#[tauri::command]
pub fn get_system_history(state: tauri::State<AppState>) -> Vec<ChatPacket> {
    // Returns ONLY System Logs
    let history = state.system_history.lock().unwrap();
    history.iter().cloned().collect()
}