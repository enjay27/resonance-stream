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
use local_ip_address::{list_afinet_netifas, local_ip};
use log::log;
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

fn setup_raw_socket(local_ip: Ipv4Addr) -> Socket {
    // 1. Create a Raw IPv4 Socket using the raw integer 0 (IPPROTO_IP)
    // This avoids the 'No associated item' error and is the standard for sniffing
    let socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::from(0)))
        .expect("Failed to create raw socket. Ensure you are running as Administrator.");

    // 2. Bind to local interface
    let address = std::net::SocketAddr::from((local_ip, 0));

    socket.bind(&address.into()).map_err(|e| {
        log::trace!("Bind Error: {:?}. IP used: {:?}", e, local_ip);
        e
    }).expect("Failed to bind to interface");

    // 3. Enable SIO_RCVALL (Windows Promiscuous Mode)
    let rcval: u32 = 1; // RCVALL_ON
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
            panic!("WSAIoctl SIO_RCVALL failed. Admin rights are mandatory.");
        }
    }

    socket
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

fn ensure_firewall_rule(app_handle: &AppHandle) {
    // 1. Get the exact path of where the user is currently running the app
    if let Ok(exe_path) = env::current_exe() {
        if let Some(path_str) = exe_path.to_str() {

            inject_system_message(app_handle, SystemLogLevel::Info, "Sniffer", "Configuring Windows Firewall for Raw Sockets...");

            // 2. CREATE_NO_WINDOW flag (0x08000000) prevents the black CMD box from flashing!
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            // 3. Run the netsh command silently
            let result = Command::new("netsh")
                .args([
                    "advfirewall", "firewall", "add", "rule",
                    "name=BPSR Translator (Inbound)",
                    "dir=in",
                    "action=allow",
                    &format!("program={}", path_str),
                    "enable=yes",
                    "profile=any"
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            match result {
                Ok(status) if status.success() => {
                    inject_system_message(app_handle, SystemLogLevel::Success, "Sniffer", "Firewall configured successfully.");
                }
                _ => {
                    inject_system_message(app_handle, SystemLogLevel::Warning, "Sniffer", "Failed to auto-configure firewall. Inbound chat may be blocked.");
                }
            }
        }
    }
}

fn start_rawsocket_parser(app_config: AppConfig, generation: u64, app: &AppHandle) {
    ensure_firewall_rule(app);

    let local_ip = find_game_interface_ip();
    let socket = setup_raw_socket(local_ip.unwrap());

    let mut buf = [0u8; 65535];
    let uninit_buf = unsafe {
        std::mem::transmute::<&mut [u8], &mut [std::mem::MaybeUninit<u8>]>(buf.as_mut_slice())
    };

    let mut streams: HashMap<[u8; 6], PacketBuffer> = HashMap::new();

    loop {
        let n = match socket.recv(uninit_buf) {
            Ok(n) => n,
            Err(_) => continue,
        };

        let packet = &buf[..n];

        // Basic sanity checks
        if n < 40 || packet[9] != 6 { continue; }

        let ip_hl = ((packet[0] & 0x0f) * 4) as usize;
        let tcp_start = ip_hl;
        if packet.len() < tcp_start + 20 { continue; }

        let src_port = u16::from_be_bytes([packet[tcp_start], packet[tcp_start + 1]]);
        let dst_port = u16::from_be_bytes([packet[tcp_start + 2], packet[tcp_start + 3]]);

        if src_port == 5003 || dst_port == 5003 {
            log::trace!("--- [PORT 5003 PACKET DETECTED] ---");

            if src_port == 5003 {
                log::trace!("🌐 DIRECTION: INBOUND (Server -> You)");
            } else {
                log::trace!("💻 DIRECTION: OUTBOUND (You -> Server)");
            }

            let ip_total_length = u16::from_be_bytes([packet[2], packet[3]]) as usize;

            if ip_total_length > n || ip_total_length < 40 {
                log::trace!("❌ DROPPED: Packet length mismatch. Read: {}, Expected: {}", n, ip_total_length);
                continue;
            }

            let exact_packet = &packet[..ip_total_length];
            let tcp_hl = ((exact_packet[tcp_start + 12] >> 4) * 4) as usize;
            let payload_offset = tcp_start + tcp_hl;

            if exact_packet.len() > payload_offset {
                log::debug!("✅ PASSED: Extracted game data, sending to parse_star_resonance...");
                feed_watchdog();
                parse_star_resonance(&app, &mut streams, Cow::from(exact_packet));
            } else {
                log::debug!("⚠️ IGNORED: No TCP Payload (Empty ACK packet)");
            }
        }
    }
}

fn start_windivert_parser(config: AppConfig, my_generation: u64, app_handle: &AppHandle) -> bool {
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
            return true;
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
            log::trace!("Raw Data: {:?}", raw_data);
            parse_star_resonance(&app_handle, &mut streams, raw_data);
        }
    }
    false
}

fn parse_star_resonance(app_handle: &&AppHandle, streams: &mut HashMap<[u8; 6], PacketBuffer>, raw_data: Cow<[u8]>) {
    // 1. Safely extract BOTH ports directly from the IP/TCP headers
    if raw_data.len() < 40 { return; }
    let ip_hl = ((raw_data[0] & 0x0f) * 4) as usize;
    if raw_data.len() < ip_hl + 4 { return; }

    let src_port = u16::from_be_bytes([raw_data[ip_hl], raw_data[ip_hl + 1]]);
    let dst_port = u16::from_be_bytes([raw_data[ip_hl + 2], raw_data[ip_hl + 3]]);

    log::trace!("[Parse] raw_data {:?}", raw_data);

    if let Some(stream_key) = parser::extract_stream_key(&*raw_data) {
        log::trace!("[Parse] stream_key {:?}", stream_key);
        if let Some(payload) = parser::extract_tcp_payload(&*raw_data) {
            log::trace!("[Parse] payload {:?}", payload);
            // 2. We already know it's a game packet from the loops, so hardcode 5003 for the application stripper
            if let Some(game_data) = parser::strip_application_header(payload, 5003) {
                log::trace!("[Parse] game_data {:?}", game_data);
                let p_buf = streams.entry(stream_key).or_insert_with(PacketBuffer::new);
                p_buf.add(game_data);

                while let Some(full_packet) = p_buf.next() {
                    // 3. Process the packet if EITHER the source or destination is 5003
                    log::trace!("[Parse] full_packet {:?}", full_packet);
                    if src_port == 5003 || dst_port == 5003 {
                        log::trace!("[Parse] inside port check {:?}", full_packet);
                        parse_and_emit_5003(&full_packet, &app_handle);
                    }
                }
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

    log::trace!("[5003] raw_payload {:?}", raw_payload);

    let events = crate::protocol::parser::stage2_process(raw_payload);

    log::trace!("[5003] events {:?}", events);

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