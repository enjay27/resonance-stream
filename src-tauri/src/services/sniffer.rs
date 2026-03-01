use crate::packet_buffer::PacketBuffer;
use crate::{get_model_path, inject_system_message, parsing_pipeline, store_and_emit, NetworkInterface, SnifferStatePayload};
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, thread};
use tauri::{AppHandle, Emitter, Manager, State, Window};

use crate::config::AppConfig;
use crate::protocol::parser::{self, Port5003Event};
use crate::protocol::types::{
    AppState, SystemLogLevel
};
use crate::services::translator::{contains_japanese, TranslationJob};
use crossbeam_channel::Sender;
use etherparse::{NetHeaders, PacketHeaders, TransportHeader};
use local_ip_address::{list_afinet_netifas, local_ip};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::Ipv4Addr;
use std::os::windows::process::CommandExt;
use std::process::Command;
use crate::services::processor::convert_to_romaji;
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
}

// --- GLOBAL STATE ---
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

    // --- WATCHDOG THREAD ---
    let app_handle_watchdog = app.clone();
    let rx_watchdog = rx.clone();
    thread::spawn(move || {
        loop {
            // Wait 5 seconds. If the sender drops during this sleep, it breaks immediately!
            match rx_watchdog.recv_timeout(Duration::from_secs(5)) {
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                _ => {} // Continue running on Timeout or Empty
            }

            let last = LAST_TRAFFIC_TIME.load(Ordering::Relaxed);
            if last == 0 { continue; }

            let start = SystemTime::now();
            let now = start.duration_since(UNIX_EPOCH).unwrap().as_secs();

            if now.saturating_sub(last) > 15 {
                inject_system_message(&app_handle_watchdog, SystemLogLevel::Warning, "Sniffer", "Watchdog: No game traffic for 15s.");
                let _ = app_handle_watchdog.emit("sniffer-status", "warning");
                feed_watchdog();
            }
        }
    });

    // --- MAIN SNIFFER THREAD ---
    let app_handle = app.clone();
    let rx_main = rx.clone();

    thread::spawn(move || {
        inject_system_message(&app_handle, SystemLogLevel::Success, "Sniffer", "Engine Active");
        emit_sniffer_state(&app_handle, "Starting", "Engine Active");

        if config.log_level.to_lowercase() == "debug" || config.log_level.to_lowercase() == "info" {
            if let Ok(network_interfaces) = list_afinet_netifas() {
                for (name, ip) in network_interfaces {
                    inject_system_message(&app_handle, SystemLogLevel::Debug, "Sniffer", format!("Active Interface: {} ({:?})", name, ip));
                }
            }
        }

        ensure_firewall_rule(&app_handle);

        let local_ip = if !config.network_interface.is_empty() {
            match config.network_interface.parse::<std::net::Ipv4Addr>() {
                Ok(ip) => {
                    inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", format!("Using manually selected Interface: {}", ip));
                    ip
                },
                Err(_) => {
                    inject_system_message(&app_handle, SystemLogLevel::Error, "Sniffer", "Invalid manual IP format. Falling back to Auto-Detect.");
                    find_game_interface_ip().unwrap_or(std::net::Ipv4Addr::new(127, 0, 0, 1))
                }
            }
        } else {
            match find_game_interface_ip() {
                Some(ip) => {
                    inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", format!("Auto-Targeting Network Interface: {}", ip));
                    emit_sniffer_state(&app_handle, "Binding", &format!("Auto-Targeting Network Interface: {}", ip));
                    ip
                },
                None => {
                    inject_system_message(&app_handle, SystemLogLevel::Error, "Sniffer", "NETWORK_ERROR: Could not find a valid local IPv4 network interface.");
                    return;
                }
            }
        };

        let socket = match setup_raw_socket(local_ip, &app_handle) {
            Ok(s) => s,
            Err(_) => return,
        };

        // [CRITICAL] Set a timeout so socket.recv doesn't permanently block thread shutdown!
        if let Err(e) = socket.set_read_timeout(Some(Duration::from_millis(500))) {
            inject_system_message(&app_handle, SystemLogLevel::Error, "Sniffer", &format!("Failed to set socket timeout: {:?}", e));
            return;
        }

        inject_system_message(&app_handle, SystemLogLevel::Success, "Sniffer", "Raw Socket active. Listening for game traffic...");
        emit_sniffer_state(&app_handle, "Active", "Listening for game traffic...");

        let mut buf = [0u8; 65535];
        let uninit_buf = unsafe {
            std::mem::transmute::<&mut [u8], &mut [std::mem::MaybeUninit<u8>]>(buf.as_mut_slice())
        };

        let mut streams: HashMap<[u8; 6], PacketBuffer> = HashMap::new();

        loop {
            // Check if the Config changed and the thread should commit suicide
            if let Err(crossbeam_channel::TryRecvError::Disconnected) = rx_main.try_recv() {
                inject_system_message(&app_handle, SystemLogLevel::Info, "Sniffer", "Sniffer thread shutting down.");
                break;
            }

            let n = match socket.recv(uninit_buf) {
                Ok(n) => n,
                // Loop back to check the channel if the 500ms timeout occurs
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => continue,
            };

            let packet = &buf[..n];
            let headers = match PacketHeaders::from_ip_slice(packet) {
                Ok(h) => h,
                Err(_) => continue,
            };

            if let Some(TransportHeader::Tcp(tcp)) = headers.transport {
                let src_port = tcp.source_port;
                if src_port == 5003 {
                    let payload = headers.payload.slice();
                    if payload.is_empty() { continue; }

                    let mut stream_key = [0u8; 6];
                    if let Some(NetHeaders::Ipv4(ipv4, _extensions)) = headers.net {
                        stream_key[0..4].copy_from_slice(&ipv4.source);
                    }
                    stream_key[4..6].copy_from_slice(&src_port.to_be_bytes());

                    feed_watchdog();
                    process_game_stream(&app_handle, &mut streams, stream_key, payload, src_port);
                }
            }
        }
    });

    tx // Return the Sender to AppState!
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
        emit_sniffer_state(app, "Error", &msg);
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

    // Aggressive blocklist for common Virtual Adapters and VPNs
    let ignore_list = [
        "Loopback", "vEthernet", "TAP", "Tailscale", "WireGuard",
        "OpenVPN", "Radmin", "Hamachi", "ZeroTier", "VMware",
        "VirtualBox", "WSL", "Npcap"
    ];

    for (name, ip) in network_interfaces {
        let name_lower = name.to_lowercase();

        // Skip if the adapter name contains any of the blocked keywords
        if ignore_list.iter().any(|&keyword| name_lower.contains(&keyword.to_lowercase())) {
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

pub fn ensure_firewall_rule(app: &AppHandle) {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(path_str) = exe_path.to_str() {
            inject_system_message(app, SystemLogLevel::Info, "Sniffer", "Configuring Windows Firewall...");
            emit_sniffer_state(app, "Firewall", "Configuring Windows Firewall...");

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

fn process_game_stream(
    app_handle: &AppHandle,
    streams: &mut HashMap<[u8; 6], PacketBuffer>,
    stream_key: [u8; 6],
    payload: &[u8],
    src_port: u16,
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
                emit_parsed_message(&full_packet, app_handle);
            }
        }
    }
}

// --- STAGE 3: EMIT ---
// Filters out duplicates and dispatches the final events to Tauri.
pub fn emit_parsed_message(data: &[u8], app: &AppHandle) {
    // If it's a server packet, this safely returns without spamming logs
    let events = parsing_pipeline(data, app);

    println!("[Emit] Events {:?}", events);

    for event in events {
        match event {
            Port5003Event::Chat(mut c) => {
                // --- 1. NICKNAME CACHE & ROMAJI SWAP ---
                if contains_japanese(&c.nickname) {
                    // Access the AppState from Tauri
                    let state = app.state::<AppState>();
                    let mut cache = state.nickname_cache.lock().unwrap();
                    inject_system_message(&app, SystemLogLevel::Trace, "Sniffer", format!("[5003 Event] check nickname cache included {:?}", c.nickname));

                    // Check cache. If miss, ask the processor to convert it!
                    let romaji = cache.entry(c.nickname.clone()).or_insert_with(|| {
                        inject_system_message(&app, SystemLogLevel::Trace, "Sniffer", format!("[5003 Event] nickname not included in cache. convert from Japanese {:?}", c.nickname));
                        convert_to_romaji(&c.nickname)
                    }).clone();

                    inject_system_message(&app, SystemLogLevel::Trace, "Sniffer", format!("[5003 Event] nickname romaji {:?}", romaji));

                    // Attach it to the struct so the UI and Preprocessor can see it
                    c.nickname_romaji = Some(romaji);
                }

                // --- 2. EMIT TO UI ---
                let current_pid = app.state::<AppState>().next_pid.fetch_add(1, Ordering::SeqCst);
                c.pid = current_pid;
                store_and_emit(app, c.clone());

                // --- 3. TRANSLATE & ARCHIVE ROUTING ---
                let config = crate::config::load_config(app.clone());
                let requires_translation = config.use_translation && contains_japanese(&c.message);

                if requires_translation {
                    // Route to Translator: The translator will archive it AFTER finishing the translation.
                    let state = app.state::<AppState>();
                    if let Some(tx) = state.translator_tx.lock().unwrap().as_ref() {
                        let _ = tx.send(crate::services::translator::TranslationJob { chat: c.clone() });
                    };
                } else if config.archive_chat {
                    // Route directly to Archive: Translation is OFF (or text isn't Japanese), but archiving is ON.
                    let state = app.state::<AppState>();
                    if let Some(df_tx) = state.data_factory_tx.lock().unwrap().as_ref() {
                        let _ = df_tx.send(crate::io::DataFactoryJob {
                            pid: current_pid,
                            original: c.message.clone(),
                            translated: None, // Explicitly no translation
                        });
                    };
                }
            }
        }
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

#[tauri::command]
pub fn get_network_interfaces() -> Vec<NetworkInterface> {
    let mut interfaces = Vec::new();
    // Assuming you have `local_ip_address` crate from your sniffer
    if let Ok(netifas) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in netifas {
            if let std::net::IpAddr::V4(ipv4) = ip {
                interfaces.push(NetworkInterface {
                    name,
                    ip: ipv4.to_string(),
                });
            }
        }
    }
    interfaces
}

pub fn emit_sniffer_state(app: &tauri::AppHandle, state: &str, message: &str) {
    let _ = app.emit("sniffer-state", SnifferStatePayload {
        state: state.to_string(),
        message: message.to_string(),
    });
}