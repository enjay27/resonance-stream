use local_ip_address::list_afinet_netifas;
use socket2::{Domain, Protocol, Socket, Type};
use std::env;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use tauri::AppHandle;

use resonance_core::sniffer_net::{
    pick_interface, rule_name_for, InterfacePick, PickRule, LEGACY_RULE_NAME,
};

use super::emit_sniffer_state;
use crate::protocol::types::{LogLevel, SnifferState, SystemLogLevel};
use crate::{inject_system_message, NetworkInterface};

const CREATE_NO_WINDOW: u32 = 0x08000000; //
const RECV_BUFFER_BYTES: usize = 4 * 1024 * 1024;
/// Any routable public address: asking the OS how it would reach it tells which
/// adapter the default route uses. Nothing is sent to it.
const ROUTE_PROBE_ADDR: Ipv4Addr = Ipv4Addr::new(8, 8, 8, 8);

// --- 3. NETWORK INITIALIZATION ---
pub fn initialize_network_socket(
    app: &AppHandle,
    config: &crate::config::AppConfig,
) -> Option<Socket> {
    if matches!(config.log_level, LogLevel::Debug | LogLevel::Info) {
        if let Ok(network_interfaces) = list_afinet_netifas() {
            for (name, ip) in network_interfaces {
                inject_system_message(
                    app,
                    SystemLogLevel::Debug,
                    "Sniffer",
                    format!("Active Interface: {} ({:?})", name, ip),
                );
            }
        }
    }

    let local_ip = if !config.network_interface.is_empty() {
        match config.network_interface.parse::<std::net::Ipv4Addr>() {
            Ok(ip) => {
                inject_system_message(
                    app,
                    SystemLogLevel::Info,
                    "Sniffer",
                    format!("Using manually selected Interface: {}", ip),
                );
                ip
            }
            Err(_) => {
                inject_system_message(
                    app,
                    SystemLogLevel::Error,
                    "Sniffer",
                    "Invalid manual IP format. Falling back to Auto-Detect.",
                );
                find_game_interface()
                    .map(|pick| pick.ip)
                    .unwrap_or(std::net::Ipv4Addr::new(127, 0, 0, 1))
            }
        }
    } else {
        match find_game_interface() {
            Some(InterfacePick { ip, rule }) => {
                let how = match rule {
                    PickRule::Route => "default route",
                    PickRule::ListOrder => "first physical adapter",
                };
                inject_system_message(
                    app,
                    SystemLogLevel::Info,
                    "Sniffer",
                    format!("Auto-Targeting Network Interface: {} ({})", ip, how),
                );
                emit_sniffer_state(
                    app,
                    SnifferState::Binding,
                    &format!("Auto-Targeting Network Interface: {} ({})", ip, how),
                );
                ip
            }
            None => {
                inject_system_message(
                    app,
                    SystemLogLevel::Error,
                    "Sniffer",
                    "NETWORK_ERROR: Could not find a valid local IPv4 network interface.",
                );
                return None;
            }
        }
    };

    let socket = setup_raw_socket(local_ip, app).ok()?;

    // Every IP packet on the interface lands in this socket; the default
    // buffer overflows (and drops chat) during bursts.
    if let Err(e) = socket.set_recv_buffer_size(RECV_BUFFER_BYTES) {
        inject_system_message(
            app,
            SystemLogLevel::Warning,
            "Sniffer",
            format!("Could not enlarge the socket receive buffer: {:?}", e),
        );
    }

    if let Err(e) = socket.set_read_timeout(Some(Duration::from_millis(500))) {
        inject_system_message(
            app,
            SystemLogLevel::Error,
            "Sniffer",
            &format!("Failed to set socket timeout: {:?}", e),
        );
        return None;
    }

    Some(socket)
}

pub fn setup_raw_socket(local_ip: Ipv4Addr, app: &AppHandle) -> Result<Socket, String> {
    // 1. Create socket safely
    let socket = match Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::from(0))) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!(
                "ACCESS_DENIED: Failed to create socket. Please run as Administrator. ({:?})",
                e
            );
            inject_system_message(app, SystemLogLevel::Error, "Sniffer", &msg);
            return Err(msg);
        }
    };

    // 2. Bind safely
    let address = std::net::SocketAddr::from((local_ip, 0));
    if let Err(e) = socket.bind(&address.into()) {
        let msg = format!(
            "BIND_FAILED: Could not bind to interface {:?}. ({:?})",
            local_ip, e
        );
        inject_system_message(app, SystemLogLevel::Error, "Sniffer", &msg);
        emit_sniffer_state(app, SnifferState::Error, &msg);
        return Err(msg);
    }

    // 3. Enable Promiscuous Mode safely
    let rcval: u32 = 1;
    let mut out_buffer = [0u8; 4];
    let mut bytes_returned: u32 = 0;
    unsafe {
        use std::os::windows::io::AsRawSocket;
        use windows_sys::Win32::Networking::WinSock::SIO_RCVALL;

        let result = windows_sys::Win32::Networking::WinSock::WSAIoctl(
            socket.as_raw_socket() as _,
            SIO_RCVALL,
            &rcval as *const _ as _,
            std::mem::size_of::<u32>() as u32,
            out_buffer.as_mut_ptr() as _,
            out_buffer.len() as u32,
            &mut bytes_returned,
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

/// The source address the OS would use to reach the internet, i.e. the adapter
/// of the default route. `None` when there is no route (offline).
fn route_source_ip() -> Option<Ipv4Addr> {
    // `connect` on a UDP socket only selects the route; no packet leaves.
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((ROUTE_PROBE_ADDR, 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) => Some(ip),
        IpAddr::V6(_) => None,
    }
}

/// The adapter to sniff on, when the user has not chosen one: the routed adapter,
/// else the first physical one (`resonance_core::sniffer_net::pick_interface`).
pub fn find_game_interface() -> Option<InterfacePick> {
    let candidates: Vec<(String, Ipv4Addr)> = list_afinet_netifas()
        .ok()?
        .into_iter()
        .filter_map(|(name, ip)| match ip {
            IpAddr::V4(ipv4) => Some((name, ipv4)),
            IpAddr::V6(_) => None,
        })
        .collect();
    pick_interface(&candidates, route_source_ip())
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

#[tauri::command]
pub fn ensure_firewall_rule_command(app: tauri::AppHandle) -> Result<String, String> {
    if crate::test_env::no_capture() {
        return Ok("Skipped (--no-capture)".to_string());
    }
    if let Ok(exe_path) = env::current_exe() {
        if let Some(path_str) = exe_path.to_str() {
            // The rule is per exe (the dev and the installed exe each get
            // their own), so only this exe's rule is replaced. The one rule
            // older versions shared between all exes is dropped.
            let rule_name = rule_name_for(path_str);
            remove_firewall_rule(&rule_name);
            remove_firewall_rule(LEGACY_RULE_NAME);

            inject_system_message(
                &app,
                SystemLogLevel::Info,
                "Sniffer",
                "Requesting Administrator privileges to configure Windows Firewall...",
            );

            // Create new rule
            let result = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    &format!("name={}", rule_name),
                    "dir=in",
                    "action=allow",
                    "protocol=TCP",
                    "remoteport=5003",
                    "remoteip=172.65.0.0/16", // Specific to BPSR servers
                    &format!("program={}", path_str),
                    "enable=yes",
                    "profile=any",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            match result {
                Ok(status) if status.success() => {
                    inject_system_message(
                        &app,
                        SystemLogLevel::Success,
                        "Sniffer",
                        "Firewall configured successfully.",
                    );
                    Ok("Success".to_string())
                }
                _ => {
                    let err = "Failed to configure firewall. The user may have denied the Administrator prompt.".to_string();
                    inject_system_message(&app, SystemLogLevel::Error, "Sniffer", &err);
                    Err(err)
                }
            }
        } else {
            Err("Failed to parse executable path.".to_string())
        }
    } else {
        Err("Could not find executable path.".to_string())
    }
}

pub fn remove_firewall_rule(rule_name: &str) {
    let _ = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={}", rule_name),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

/// Whether the firewall has the rule for the running exe. A rule made for
/// another exe (the dev build while this is the installed one, or the reverse)
/// does not count: it would not let this exe receive the game's packets.
pub fn check_firewall_rule() -> bool {
    let Some(exe_path) = env::current_exe().ok() else {
        return false;
    };
    let Some(path_str) = exe_path.to_str() else {
        return false;
    };
    let rule_name = rule_name_for(path_str);
    let result = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            &format!("name={}", rule_name),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match result {
        Ok(output) => output.status.success(), // Returns true if the rule exists
        Err(_) => false,
    }
}
