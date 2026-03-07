mod stream_traacker;
mod message_processor;
mod pipeline;

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

// --- GLOBAL STATE ---
static LAST_TRAFFIC_TIME: AtomicU64 = AtomicU64::new(0);

const CREATE_NO_WINDOW: u32 = 0x08000000;
const RULE_NAME: &str = "Resonance Stream (Packet Sniffing)";

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
        let mut pipeline = crate::services::sniffer::pipeline::ChatPipeline::new();

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

// --- 3. NETWORK INITIALIZATION ---
fn initialize_network_socket(app: &AppHandle, config: &crate::config::AppConfig) -> Option<Socket> {
    if config.log_level.to_lowercase() == "debug" || config.log_level.to_lowercase() == "info" {
        if let Ok(network_interfaces) = list_afinet_netifas() {
            for (name, ip) in network_interfaces {
                inject_system_message(app, SystemLogLevel::Debug, "Sniffer", format!("Active Interface: {} ({:?})", name, ip));
            }
        }
    }

    ensure_firewall_rule(app);

    let local_ip = if !config.network_interface.is_empty() {
        match config.network_interface.parse::<std::net::Ipv4Addr>() {
            Ok(ip) => {
                inject_system_message(app, SystemLogLevel::Info, "Sniffer", format!("Using manually selected Interface: {}", ip));
                ip
            },
            Err(_) => {
                inject_system_message(app, SystemLogLevel::Error, "Sniffer", "Invalid manual IP format. Falling back to Auto-Detect.");
                find_game_interface_ip().unwrap_or(std::net::Ipv4Addr::new(127, 0, 0, 1))
            }
        }
    } else {
        match find_game_interface_ip() {
            Some(ip) => {
                inject_system_message(app, SystemLogLevel::Info, "Sniffer", format!("Auto-Targeting Network Interface: {}", ip));
                emit_sniffer_state(app, "Binding", &format!("Auto-Targeting Network Interface: {}", ip));
                ip
            },
            None => {
                inject_system_message(app, SystemLogLevel::Error, "Sniffer", "NETWORK_ERROR: Could not find a valid local IPv4 network interface.");
                return None;
            }
        }
    };

    let socket = setup_raw_socket(local_ip, app).ok()?;

    if let Err(e) = socket.set_read_timeout(Some(Duration::from_millis(500))) {
        inject_system_message(app, SystemLogLevel::Error, "Sniffer", &format!("Failed to set socket timeout: {:?}", e));
        return None;
    }

    Some(socket)
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