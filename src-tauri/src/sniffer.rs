use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use byteorder::{LittleEndian, ReadBytesExt};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use windivert::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, Window};
use crate::{inject_system_message, store_and_emit};
use crate::packet_buffer::{PacketBuffer, read_varint_safe};

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
    pub sequence_id: u64,
    pub message: String,
    #[serde(default)]
    pub translated: Option<String>,
    #[serde(default)]
    pub nickname_romaji: Option<String>,
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

pub struct AppState {
    pub tx: Mutex<Option<tauri_plugin_shell::process::CommandChild>>,
    pub chat_history: Mutex<IndexMap<u64, ChatPacket>>,
    pub system_history: Mutex<VecDeque<SystemMessage>>,
    pub next_pid: AtomicU64,
    pub nickname_cache: Mutex<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityStatePacket {
    pub pid: u64,
    pub entity_id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub level: u64,      // Field 18
    pub mount_id: u64,   // Field 25
    pub hp_percent: u64, // Field 15
    pub state_id: u64,   // Field 13
    pub anim_speed: f64,   // Field 20 (Wire 1/4)
    pub target_id: u64,    // Field 34
    pub zone_id: u64,      // Field 35
    pub status_code: u64, // Extracted from Field 10/11 logic
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ZoneRecruitment {
    pub pid: u64,
    pub nickname: String,
    pub level: u64,
    pub message: String,      // The recruitment text
    pub class_id: u64,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LobbyRecruitment {
    pub leader_nickname: String,
    pub description: String,
    pub member_count: u32,
    pub max_members: u32,
    pub level_requirement: u32,
    pub party_id: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartyLobbySnapshot {
    pub total_parties: u32,
    pub entries: Vec<LobbyRecruitment>,
    pub timestamp: u64,
}

lazy_static! {
    // Tracks already seen fields to prevent log flooding
    static ref DISCOVERED_FIELDS: Mutex<HashSet<u32>> = Mutex::new(HashSet::from([
        1, 2, 3, 7, 10, 11, 13, 15, 18, 20, 25, 34, 35
    ]));

    static ref DISCOVERED_FIELDS_5003: Mutex<HashSet<u32>> = Mutex::new(HashSet::from([
        0, 1, 2, 3, 4, 5, 6, 7, 12, 16, 17, 21, 22, 23, 25, 26, 29, 31
    ]));
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
            // If the global generation has changed, I am obsolete.
            if SNIFFER_GENERATION.load(Ordering::Relaxed) != my_generation {
                inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", format!("Sniffer Gen {} Shutdown signal received. Exiting.", my_generation));
                break; // This drops 'wd', closing the handle cleanly.
            }

            if let Ok(packet) = wd.recv(Some(&mut buffer)) {
                if config.is_debug && LAST_TRAFFIC_TIME.load(Ordering::Relaxed) == 0 {
                    inject_system_message(&app_handle, SystemLogLevel::Success, "Sniffer", "First Packet Captured! Network link established.");
                }
                // Feed the Watchdog because we saw traffic
                feed_watchdog();

                let raw_data = packet.data;

                // 1. Get the Stream Key
                if let Some(stream_key) = extract_stream_key(&*raw_data) {
                    // [NEW] Log new connection detection (IP/Port)
                    if config.is_debug && !streams.contains_key(&stream_key) {
                        let src_ip = format!("{}.{}.{}.{}", stream_key[0], stream_key[1], stream_key[2], stream_key[3]);
                        let src_port = u16::from_be_bytes([stream_key[4], stream_key[5]]);
                        // inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", format!("New Stream Detected: {}:{}", src_ip, src_port));
                    }

                    let port = u16::from_be_bytes([stream_key[4], stream_key[5]]);

                    // 2. Extract Payload
                    if let Some(payload) = extract_tcp_payload(&*raw_data) {
                        if let Some(game_data) = strip_application_header(payload, port) {
                            let p_buf = streams.entry(stream_key).or_insert_with(PacketBuffer::new);
                            p_buf.add(game_data);

                            while let Some(full_packet) = p_buf.next() {
                                if port == 10250 {
                                    if let Some(state) = parse_entity_state(&full_packet, &app_handle) {
                                        let _ = app_handle.emit("entity-state-event", state);
                                    }
                                } else if port == 5003 {
                                    // Try Chat first, then Party, then Broadcasts
                                    println!("payload {:?}", full_packet);
                                    if let Some(chat) = parse_port_5003(&full_packet, &app) {
                                        store_and_emit(&app_handle, chat);
                                    }
                                    if let Some(lobby) = parse_party_lobby(&full_packet, &app_handle) {
                                        println!("parse party lobby {:?}", lobby);
                                        let _ = app_handle.emit("party-lobby-update", lobby.clone());
                                        crate::inject_system_message(
                                            &app_handle,
                                            SystemLogLevel::Info,
                                            "Lobby",
                                            format!("Lobby Refresh: {} parties found.", lobby.total_parties)
                                        );
                                    }
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
pub fn parse_entity_state(data: &[u8], app: &AppHandle) -> Option<EntityStatePacket> {
    if data.len() < 5 || data[0] != 0x0A { return None; }

    // Fast-fail for synchronization stubs
    if data.ends_with(&[26, 0]) && data.len() < 30 { return None; }

    let mut state = EntityStatePacket::default();
    let (total_len, header_read) = read_varint(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    while i < safe_end {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // Entity ID
                let (val, read) = read_varint(&data[i..safe_end]);
                state.entity_id = val;
                i += read;
            }
            2 => { // Position Container
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);
                let mut sub_i = i;
                while sub_i < block_end {
                    let sub_tag = data[sub_i];
                    sub_i += 1;
                    if (sub_tag & 0x07) == 5 { // 32-bit Float
                        if let Ok(v) = (&data[sub_i..]).read_f32::<LittleEndian>() {
                            match sub_tag >> 3 { 1 => state.x = v, 2 => state.y = v, 3 => state.z = v, _ => {} }
                        }
                        sub_i += 4;
                    } else { sub_i += skip_field(sub_tag & 0x07, &data[sub_i..block_end]); }
                }
                i = block_end;
            }
            20 => { // Animation Speed / Scale (Fixed64)
                if let Ok(v) = (&data[i..safe_end]).read_f64::<LittleEndian>() { state.anim_speed = v; }
                i += 8;
            }
            7 | 10 | 11 => { // Nested Metadata Containers
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                // Currently skipping internal structure but silencing the warning
                i = (i + len as usize).min(safe_end);
            }
            13 | 15 | 18 | 25 | 34 | 35 => { // Varint Metadata
                let (val, read) = read_varint(&data[i..safe_end]);
                match field_num {
                    13 => state.state_id = val,
                    15 => state.hp_percent = val,
                    18 => state.level = val,
                    25 => state.mount_id = val,
                    34 => state.target_id = val,
                    35 => state.zone_id = val,
                    _ => {}
                }
                i += read;
            }
            unknown => {
                log_missing_field(app, unknown as u32, wire_type, data);
                i += skip_field(wire_type, &data[i..safe_end]);
            }
        }
    }
    state.timestamp = chrono::Utc::now().timestamp_millis() as u64;
    Some(state)
}

fn strip_application_header(payload: &[u8], port: u16) -> Option<&[u8]> {
    if payload.len() < 5 { return None; }

    return match port {
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
fn log_missing_field(app: &tauri::AppHandle, field_num: u32, wire_type: u8, data: &[u8]) {
    let mut discovered = DISCOVERED_FIELDS.lock().unwrap();
    if discovered.insert(field_num) {
        crate::inject_system_message(
            app,
            SystemLogLevel::Warning,
            "Discovery",
            format!("New Field #{} (Wire {}). Packet: {:?}", field_num, wire_type, data)
        );
    }
}

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

pub fn parse_zone_broadcast(data: &[u8]) -> Option<ZoneRecruitment> {
    if data.len() < 15 || data[0] != 0x0A { return None; }

    let mut recruit = ZoneRecruitment::default();
    let (total_len, header_read) = read_varint(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    while i < safe_end {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            2 => { // Player Info Container (Nickname/Level)
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);
                // Reuse existing profile parser
                let mut temp_chat = ChatPacket::default();
                parse_user_container(&data[i..block_end], &mut temp_chat);
                recruit.nickname = temp_chat.nickname;
                recruit.level = temp_chat.level;
                recruit.class_id = temp_chat.class_id;
                i = block_end;
            }
            4 | 5 => { // System Broadcast Fields (Recruitment Text)
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);
                if let Some(msg_bytes) = data.get(i..block_end) {
                    recruit.message = String::from_utf8_lossy(msg_bytes).into_owned();
                }
                i = block_end;
            }
            _ => i += skip_field(wire_type, &data[i..safe_end]),
        }
    }

    if !recruit.message.is_empty() {
        recruit.timestamp = chrono::Utc::now().timestamp_millis() as u64;
        Some(recruit)
    } else {
        None
    }
}

pub fn parse_port_5003(data: &[u8], app: &AppHandle) -> Option<ChatPacket> {
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
            3 => { // [NEW] Top-level Message or Nested Container
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);
                if let Some(sub_data) = data.get(i..block_end) {
                    // Check if this container itself has a string at tag 0x1A
                    if let Some(msg) = find_string_by_tag(sub_data, 0x1A) {
                        chat.message = msg;
                    }
                }
                i = block_end;
            }
            4 => { // Broadcast/Recruitment Block
                let (len, read) = read_varint(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);
                if let Some(sub_data) = data.get(i..block_end) {
                    if let Some(msg) = find_string_by_tag(sub_data, 0x1A) {
                        chat.message = msg;
                        if let Some(chan_id) = find_int_by_tag(sub_data, 0x10) {
                            chat.channel = match chan_id {
                                3 => "PARTY".into(), 4 => "GUILD".into(), _ => chat.channel
                            };
                        }
                    }
                }
                i = block_end;
            }
            22 => {
                // Field 22 (Wire 0): Varint
                let (_, read) = read_varint(&data[i..safe_end]);
                i += read;
            }
            0 | 5 | 6 | 7 | 12 | 16 | 17 | 21 | 23 | 25 | 26 | 29 | 31 => {
                i += skip_field(wire_type, &data[i..safe_end]);
            }
            unknown_field => {
                log_missing_field_5003(app, unknown_field as u32, wire_type, data);
                i += skip_field(wire_type, &data[i..safe_end]);
            }
        }
    }

    if !chat.message.is_empty() && chat.uid > 0 { Some(chat) } else { None }
}

pub fn parse_party_lobby(data: &[u8], app: &tauri::AppHandle) -> Option<PartyLobbySnapshot> {
    // If the data is compressed, standard Protobuf parsing will fail here.
    // However, if it's just a long list of messages, we can iterate through them.
    if data.len() < 100 || data[0] != 0x0A { return None; }

    let mut snapshot = PartyLobbySnapshot::default();
    let (total_len, header_read) = read_varint_safe(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    while i < safe_end {
        let tag = data[i];
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            4 => { // [REPEATED] The Recruitment Entry Block
                let (len, read) = read_varint_safe(&data[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);

                if let Some(sub_data) = data.get(i..block_end) {
                    let mut entry = LobbyRecruitment::default();
                    // Extract fields like description (Tag 0x1A) and counts
                    if let Some(msg) = find_string_by_tag(sub_data, 0x1A) {
                        entry.description = msg;
                        snapshot.entries.push(entry);
                    }
                }
                i = block_end;
            }
            _ => i += skip_field(tag & 0x07, &data[i..safe_end]),
        }
    }

    if !snapshot.entries.is_empty() {
        snapshot.total_parties = snapshot.entries.len() as u32;
        snapshot.timestamp = chrono::Utc::now().timestamp_millis() as u64;
        Some(snapshot)
    } else {
        None
    }
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
pub fn get_system_history(state: tauri::State<AppState>) -> Vec<SystemMessage> {
    // Change: Returns specialized SystemMessages
    let history = state.system_history.lock().unwrap();
    history.iter().cloned().collect()
}