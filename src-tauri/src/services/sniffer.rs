use std::borrow::Cow;
use crate::packet_buffer::PacketBuffer;
use crate::{inject_system_message, store_and_emit};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::{env, thread};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State, Window};
use windivert::prelude::*;

use socket2::{Socket, Domain, Type, Protocol};
use std::net::Ipv4Addr;
use std::os::windows::process::CommandExt;
use std::process::Command;
use etherparse::{PacketHeaders, TransportHeader, NetHeaders};
use local_ip_address::{list_afinet_netifas, local_ip};
use crate::config::AppConfig;
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

const CREATE_NO_WINDOW: u32 = 0x08000000;
const RULE_NAME: &str = "BPSR Translator (Game Data)";

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

fn get_active_ip() -> Option<std::net::Ipv4Addr> {
    match local_ip() {
        Ok(ip) => {
            if let std::net::IpAddr::V4(ipv4) = ip {
                Some(ipv4)
            } else {
                None // Skip IPv6 for Raw Socket SIO_RCVALL
            }
        },
        Err(_) => None,
    }
}

fn setup_raw_socket(local_ip: Ipv4Addr, app: &AppHandle) -> Result<Socket, String> {
    // 1. Create socket safely
    let socket = match Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::from(0))) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("ACCESS_DENIED: Failed to create socket. Please run as Administrator. ({:?})", e);
            inject_system_message(app, SystemLogLevel::Error, "Sniffer", &msg);
            return Err(msg);
        }
    };

    // 2. Bind safely
    let address = std::net::SocketAddr::from((local_ip, 0));
    if let Err(e) = socket.bind(&address.into()) {
        let msg = format!("BIND_FAILED: Could not bind to interface {:?}. ({:?})", local_ip, e);
        inject_system_message(app, SystemLogLevel::Error, "Sniffer", &msg);
        return Err(msg);
    }

    // 3. Enable Promiscuous Mode safely
    let rcval: u32 = 1;
    let mut out_buffer = [0u8; 4];
    unsafe {
        use windows_sys::Win32::Networking::WinSock::SIO_RCVALL;
        use std::os::windows::io::AsRawSocket;

        let result = windows_sys::Win32::Networking::WinSock::WSAIoctl(
            socket.as_raw_socket() as _,
            SIO_RCVALL,
            &rcval as *const _ as _,
            std::mem::size_of::<u32>() as u32,
            out_buffer.as_mut_ptr() as _,
            out_buffer.len() as u32,
            &mut 0,
            std::ptr::null_mut(),
            None,
        );
        if result != 0 {
            let msg = "PROMISCUOUS_MODE_FAILED: Network adapter rejected SIO_RCVALL. Admin rights required.".to_string();
            inject_system_message(app, SystemLogLevel::Error, "Sniffer", &msg);
            return Err(msg);
        }
    }

    Ok(socket)
}

pub fn find_game_interface_ip() -> Option<std::net::Ipv4Addr> {
    let network_interfaces = list_afinet_netifas().ok()?;

    for (name, ip) in network_interfaces {
        // Ignore loopback and virtual adapter common names
        if name.contains("Loopback") || name.contains("vEthernet") {
            continue;
        }

        if let std::net::IpAddr::V4(ipv4) = ip {
            // Usually, your main LAN IP starts with 192, 10, or 172
            if !ipv4.is_loopback() && !ipv4.is_link_local() {
                return Some(ipv4);
            }
        }
    }
    None
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

        start_rawsocket_parser(config, my_generation, &app);

        inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", "Old Sniffer Thread Terminated.");
    });
}

pub fn ensure_firewall_rule(app: &AppHandle) {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(path_str) = exe_path.to_str() {
            inject_system_message(app, SystemLogLevel::Info, "Sniffer", "Configuring Windows Firewall...");

            let _ = Command::new("netsh")
                .args(["advfirewall", "firewall", "delete", "rule", &format!("name={}", RULE_NAME)])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            let result = Command::new("netsh")
                .args([
                    "advfirewall", "firewall", "add", "rule",
                    &format!("name={}", RULE_NAME),
                    "dir=in",
                    "action=allow",
                    "protocol=TCP",
                    "remoteport=5003",
                    "remoteip=172.65.0.0/16",
                    &format!("program={}", path_str),
                    "enable=yes",
                    "profile=any"
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            match result {
                Ok(status) if status.success() => {
                    inject_system_message(app, SystemLogLevel::Success, "Sniffer", "Firewall configured successfully.");
                }
                _ => {
                    inject_system_message(app, SystemLogLevel::Error, "Sniffer", "Failed to configure firewall. Inbound chat may be blocked.");
                }
            }
        }
    }
}

fn start_rawsocket_parser(app_config: AppConfig, generation: u64, app: &AppHandle) {
    ensure_firewall_rule(app);

    // 1. Gracefully handle missing IP
    let local_ip = match find_game_interface_ip() {
        Some(ip) => {
            inject_system_message(app, SystemLogLevel::Info, "Sniffer", format!("Targeting Network Interface: {}", ip));
            ip
        },
        None => {
            inject_system_message(app, SystemLogLevel::Error, "Sniffer", "NETWORK_ERROR: Could not find a valid local IPv4 network interface.");
            IS_SNIFFER_RUNNING.store(false, Ordering::SeqCst);
            return;
        }
    };

    // 2. Gracefully handle socket setup
    let socket = match setup_raw_socket(local_ip, app) {
        Ok(s) => s,
        Err(_) => {
            IS_SNIFFER_RUNNING.store(false, Ordering::SeqCst);
            return; // Exit thread cleanly
        }
    };

    inject_system_message(app, SystemLogLevel::Success, "Sniffer", "Raw Socket active. Listening for game traffic...");

    let mut buf = [0u8; 65535];
    let uninit_buf = unsafe {
        std::mem::transmute::<&mut [u8], &mut [std::mem::MaybeUninit<u8>]>(buf.as_mut_slice())
    };

    let mut streams: HashMap<[u8; 6], PacketBuffer> = HashMap::new();

    loop {
        // [CRITICAL] COOPERATIVE SHUTDOWN
        // If the user restarts the sniffer, the generation changes, and this thread will exit!
        if SNIFFER_GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }

        let n = match socket.recv(uninit_buf) {
            Ok(n) => n,
            Err(_) => continue,
        };

        // 1. Let etherparse handle all the dangerous network parsing!
        let packet = &buf[..n];
        let headers = match PacketHeaders::from_ip_slice(packet) {
            Ok(h) => h,
            Err(_) => continue, // Drop malformed IP packets safely
        };

        // 2. Validate TCP and get ports
        if let Some(TransportHeader::Tcp(tcp)) = headers.transport {
            let src_port = tcp.source_port;

            if src_port == 5003 {
                let payload = headers.payload.slice();

                // Ignore empty ACK packets
                if payload.is_empty() { continue; }

                // 3. Extract the Stream Key (Source IP + Source Port)
                // to handle split messages cleanly across different players
                let mut stream_key = [0u8; 6];
                if let Some(NetHeaders::Ipv4(ipv4, _extensions)) = headers.net {
                    stream_key[0..4].copy_from_slice(&ipv4.source);
                }
                stream_key[4..6].copy_from_slice(&src_port.to_be_bytes());

                inject_system_message(&app, SystemLogLevel::Debug, "Sniffer", format!("✅ PASSED: Extracted {} bytes of game data", payload.len()));
                feed_watchdog();

                // Pass clean data to the game parser
                process_game_stream(app, &mut streams, stream_key, payload, src_port);
            }
        }
    }
}

fn process_game_stream(
    app_handle: &AppHandle,
    streams: &mut HashMap<[u8; 6], PacketBuffer>,
    stream_key: [u8; 6],
    payload: &[u8],
    src_port: u16
) {
    // 1. Strip the 5003 application header
    inject_system_message(&app_handle, SystemLogLevel::Debug, "Sniffer", format!("process game stream {:?}", payload));
    if let Some(game_data) = parser::strip_application_header(payload, 5003) {
        inject_system_message(&app_handle, SystemLogLevel::Debug, "Sniffer", format!("game data {:?}", game_data));
        // 2. Append the bytes to the correct player/server stream buffer
        let p_buf = streams.entry(stream_key).or_insert_with(PacketBuffer::new);
        p_buf.add(game_data);

        // 3. Extract fully assembled Protobuf packets
        while let Some(full_packet) = p_buf.next() {
            // We double-check the port to ensure we only emit 5003 data
            inject_system_message(&app_handle, SystemLogLevel::Debug, "Sniffer", format!("full packet {:?}", full_packet));
            if src_port == 5003 {
                inject_system_message(&app_handle, SystemLogLevel::Debug, "Sniffer", "request parse for 5003 port packet");
                parse_and_emit_5003(&full_packet, app_handle);
            }
        }
    }
}

// --- STAGE 3: EMIT ---
// Filters out duplicates and dispatches the final events to Tauri.
pub fn parse_and_emit_5003(data: &[u8], app: &AppHandle) {
    // If it's a server packet, this safely returns without spamming logs
    let raw_payload = match crate::protocol::parser::stage1_split(data) {
        Some(p) => p,
        None => return,
    };

    inject_system_message(&app, SystemLogLevel::Debug, "Sniffer", format!("[5003] stage 1 completed {:?}", raw_payload));

    let events = crate::protocol::parser::stage2_process(raw_payload);

    inject_system_message(&app, SystemLogLevel::Debug, "Sniffer", format!("[5003] stage 2 completed {:?}", events));

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

pub fn remove_firewall_rule() {
    let _ = Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule", &format!("name={}", RULE_NAME)])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}