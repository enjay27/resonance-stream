use crate::packet_buffer::{read_varint_safe, PacketBuffer};
use crate::{inject_system_message, store_and_emit};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State, Window};
use windivert::prelude::*;

// --- DATA STRUCTURES ---
// 1. Standard Chat: Focuses on player communication
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub pid: u64,
    pub channel: String,
    pub nickname: String,
    pub message: String,
    pub timestamp: u64,
    pub uid: u64,
    pub class_id: u64,
    pub level: u64,
    pub sequence_id: u64,
    // --- Translation Support ---
    #[serde(default)]
    pub translated: Option<String>,
    #[serde(default)]
    pub nickname_romaji: Option<String>,
}

// 2. Lobby Recruitment: Detailed recruitment board data
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LobbyRecruitment {
    pub party_id: u64,
    pub leader_nickname: String,
    pub description: String,      // The full Japanese description
    pub recruit_id: String,       // "ID:XXXXX" extracted from description
    pub member_count: u32,
    pub max_members: u32,
    pub timestamp: u64,
    // --- Translation Support ---
    #[serde(default)]
    pub translated: Option<String>,      // Translated party description
    #[serde(default)]
    pub nickname_romaji: Option<String>, // Leader's name in Romaji
}

// 3. Profile Assets: Player thumbnails and full renders
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAsset {
    pub uid: u64,
    pub snapshot_url: String,   // Thumbnail URL
    pub halflength_url: String, // Full render URL
    pub status_text: String,    // Original Japanese status
    pub timestamp: u64,
    // --- Translation Support ---
    #[serde(default)]
    pub translated: Option<String>, // Translated status/title
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemMessage {
    pub pid: u64,             // Unique ID for Leptos 'For' loop keys
    pub timestamp: u64,       // Milliseconds for sorting
    pub level: String,        // "info", "warn", "error", "success"
    pub source: String,       // "Backend", "Sniffer", "Translator"
    pub message: String,      // The actual log text
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SystemLogLevel {
    Info,    // Normal initialization logs
    Warning, // Sniffer not active, GPU memory low
    Error,   // Driver init failed, Sidecar crashed
    Success, // Dictionary updated, Model ready
    Debug,   // high-frequency, technical events
}

#[derive(Debug)]
pub struct SplitPayload {
    pub channel: String,
    pub chat_blocks: Vec<(u32, Vec<u8>)>,
    pub recruit_asset_blocks: Vec<Vec<u8>>,
}

pub struct AppState {
    pub tx: Mutex<Option<tauri_plugin_shell::process::CommandChild>>,
    pub chat_history: Mutex<IndexMap<u64, ChatMessage>>,
    pub system_history: Mutex<VecDeque<SystemMessage>>,
    pub next_pid: AtomicU64,
    pub nickname_cache: Mutex<HashMap<String, String>>,
}

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

                if let Some(stream_key) = extract_stream_key(&*raw_data) {
                    let port = u16::from_be_bytes([stream_key[4], stream_key[5]]);

                    if let Some(payload) = extract_tcp_payload(&*raw_data) {
                        if let Some(game_data) = strip_application_header(payload, port) {
                            let p_buf = streams.entry(stream_key).or_insert_with(PacketBuffer::new);
                            p_buf.add(game_data);

                            while let Some(full_packet) = p_buf.next() {
                                if port == 5003 {
                                    // Try Chat first, then Party, then Broadcasts
                                    println!("Payload {:?}", full_packet);
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

// ==========================================
// PARSING & UTILITIES (Keep your existing functions below)
// ==========================================
fn strip_application_header(payload: &[u8], port: u16) -> Option<&[u8]> {
    if payload.len() < 5 { return None; }

    match port {
        10250 => {
            if payload.len() > 32 && payload[32] == 0x0A { Some(&payload[32..]) } else { None }
        },
        5003 => {
            // Search for the 0x0A that correctly describes the rest of the payload
            for i in 0..payload.len().saturating_sub(3) {
                if payload[i] == 0x0A {
                    let (msg_len, varint_size) = read_varint_safe(&payload[i+1..]);
                    // If this 0x0A + its length exactly matches the end of the TCP packet, it's real
                    if varint_size > 0 && (i + 1 + varint_size + msg_len as usize) == payload.len() {
                        return Some(&payload[i..]);
                    }
                }
            }
            None
        },
        _ => if payload[0] == 0x0A { Some(payload) } else { None }
    }
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

    let (total_len, header_read) = read_varint(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    while i < safe_end {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = (tag >> 3) as u32;
        i += 1;

        let start = i;
        let consumed = skip_field(wire_type, &data[i..safe_end]);
        i += consumed;
        let end = i.min(safe_end);

        if let Some(payload) = data.get(start..end) {
            // Use entry to handle repeated fields if necessary
            fields.insert(field_num, payload.to_vec());
        }
    }
    fields
}

#[derive(Debug)]
pub enum Port5003Event {
    Chat(ChatMessage),
    Recruit(LobbyRecruitment),
    Asset(ProfileAsset),
}

// --- STAGE 1: SPLIT ---
// Separates the raw Protobuf packet into categorized byte blocks.
fn stage1_split(data: &[u8]) -> Option<SplitPayload> {
    let mut payload = SplitPayload {
        channel: "WORLD".to_string(),
        chat_blocks: Vec::new(),
        recruit_asset_blocks: Vec::new(),
    };

    if data.len() < 3 || data[0] != 0x0A { return None; }

    let (total_len, header_read) = read_varint(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    let mut is_valid_chat_packet = false;

    while i < safe_end {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = (tag >> 3) as u32;
        i += 1; // Move past the tag byte

        if wire_type == 2 {
            let (len, read) = read_varint(&data[i..safe_end]);
            i += read;
            let block_end = (i + len as usize).min(safe_end);

            if let Some(sub_data) = data.get(i..block_end) {
                match field_num {
                    2 | 3 | 4 => payload.chat_blocks.push((field_num, sub_data.to_vec())),
                    18 => payload.recruit_asset_blocks.push(sub_data.to_vec()),
                    _ => {}
                }
            }
            i = block_end; // Advance the pointer past this block
        } else if wire_type == 0 {
            let (val, read) = read_varint(&data[i..safe_end]);
            if field_num == 1 {
                // Real Chat/Recruit packets ALWAYS have a Channel ID varint here.
                is_valid_chat_packet = true;
                payload.channel = match val {
                    2 => "LOCAL".into(), 3 => "PARTY".into(), 4 => "GUILD".into(), _ => "WORLD".into(),
                };
            }
            i += read;
        } else {
            // Safely skip any other data types
            i += skip_field(wire_type, &data[i..safe_end]);
        }
    }

    // Filter out Server/Metadata packets immediately
    if is_valid_chat_packet {
        Some(payload)
    } else {
        None
    }
}

// --- STAGE 2: PROCESS ---
// Applies strict, field-mapped parsing logic to generate specific Events.
fn stage2_process(raw: SplitPayload) -> Vec<Port5003Event> {
    let mut events = Vec::new();

    // Context memory for Field 18 (which relies on Field 2 for names/IDs)
    let mut ctx_uid = 0;
    let mut ctx_nickname = String::new();
    let mut ctx_timestamp = 0;

    // 1. Process Chat Blocks
    for (field_num, block) in raw.chat_blocks {
        let mut chat = ChatMessage { channel: raw.channel.clone(), ..Default::default() };

        match field_num {
            2 | 3 => {
                parse_user_container(&block, &mut chat); // Strict parsing
            }
            4 => {
                if let Some(msg) = find_string_by_tag(&block, 0x1A) {
                    chat.message = msg;
                    if let Some(chan_id) = find_int_by_tag(&block, 0x10) {
                        chat.channel = match chan_id { 3 => "PARTY".into(), 4 => "GUILD".into(), _ => chat.channel };
                    }
                }
            }
            _ => {}
        }

        // Save context if we found player identity
        if chat.uid > 0 { ctx_uid = chat.uid; }
        if !chat.nickname.is_empty() { ctx_nickname = chat.nickname.clone(); }
        if chat.timestamp > 0 { ctx_timestamp = chat.timestamp; }

        if !chat.message.is_empty() && chat.uid > 0 {
            events.push(Port5003Event::Chat(chat));
        }
    }

    // 2. Process Recruit & Asset Blocks
    for block in raw.recruit_asset_blocks {
        let text = String::from_utf8_lossy(&block).into_owned();

        if text.contains("ID:") {
            events.push(Port5003Event::Recruit(LobbyRecruitment {
                recruit_id: text.split("ID:").nth(1).unwrap_or("").to_string(),
                leader_nickname: ctx_nickname.clone(),
                description: text,
                timestamp: ctx_timestamp,
                ..Default::default()
            }));
        } else if text.contains("https://") {
            let mut asset = ProfileAsset { uid: ctx_uid, timestamp: ctx_timestamp, ..Default::default() };
            if text.contains("snapshot") { asset.snapshot_url = text; }
            else { asset.halflength_url = text; }
            events.push(Port5003Event::Asset(asset));
        }
    }

    events
}

// --- STAGE 3: EMIT ---
// Filters out duplicates and dispatches the final events to Tauri.
pub fn parse_and_emit_5003(data: &[u8], app: &AppHandle) {
    // If it's a server packet, this safely returns without spamming logs
    let raw_payload = match stage1_split(data) {
        Some(p) => p,
        None => return,
    };

    let events = stage2_process(raw_payload);

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

// --- STRICT MAPPED PARSERS (From previous_sniffer.rs) ---

fn parse_user_container(data: &[u8], chat: &mut ChatMessage) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        i += 1; // Advance past the tag

        // Match on the EXACT tag byte, not just the field number
        match tag {
            8 => { // Tag 8 = Field 1, Wire 0 (Session ID)
                let (val, read) = read_varint(&data[i..]);
                chat.pid = val;
                i += read;
            }
            18 => { // Tag 18 = Field 2, Wire 2 (Profile Block)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    parse_profile_block(sub_data, chat);
                }
                i = block_end;
            }
            24 => { // Tag 24 = Field 3, Wire 0 (Timestamp)
                let (val, read) = read_varint(&data[i..]);
                chat.timestamp = val;
                i += read;
            }
            34 => { // Tag 34 = Field 4, Wire 2 (Message Block)
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
            // If we see Tag 32 (Field 4, Wire 0), it safely falls through to skip_field!
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
}

fn parse_profile_block(data: &[u8], chat: &mut ChatMessage) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        i += 1;

        match tag {
            8 => { // Tag 8 = Field 1, Wire 0 (Permanent UID)
                let (val, read) = read_varint(&data[i..]);
                chat.uid = val;
                i += read;
            }
            18 => { // Tag 18 = Field 2, Wire 2 (Nickname)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    chat.nickname = String::from_utf8_lossy(sub_data).into_owned();
                }
                i = block_end;
            }
            24 => { // Tag 24 = Field 3, Wire 0 (Class ID)
                let (val, read) = read_varint(&data[i..]);
                chat.class_id = val;
                i += read;
            }
            32 => { // Tag 32 = Field 4, Wire 0 (Status Flag)
                let (_, read) = read_varint(&data[i..]);
                i += read;
            }
            40 => { // Tag 40 = Field 5, Wire 0 (Level)
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

fn read_varint(data: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    let mut pos = 0;
    while pos < data.len() {
        let byte = data[pos];
        if shift >= 64 { return (value, pos); }
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
pub fn get_chat_history(state: tauri::State<AppState>) -> Vec<ChatMessage> {
    // Returns ONLY Game Chat
    let history = state.chat_history.lock().unwrap();
    history.values().cloned().collect()
}

#[tauri::command]
pub fn get_system_history(state: tauri::State<AppState>) -> Vec<SystemMessage> {
    // Change: Returns specialized SystemMessages
    let history = state.system_history.lock().unwrap();
    history.iter().cloned().collect()
}