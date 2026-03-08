use local_ip_address::list_afinet_netifas;
use socket2::{Domain, Protocol, Socket, Type};
use std::env;
use std::net::Ipv4Addr;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use tauri::AppHandle;

use super::emit_sniffer_state;
use crate::protocol::types::SystemLogLevel;
use crate::{inject_system_message, NetworkInterface};

const CREATE_NO_WINDOW: u32 = 0x08000000; //
const RULE_NAME: &str = "Resonance Stream (Packet Sniffing)"; //

// --- 3. NETWORK INITIALIZATION ---
pub fn initialize_network_socket(
    app: &AppHandle,
    config: &crate::config::AppConfig,
) -> Option<Socket> {
    if config.log_level.to_lowercase() == "debug" || config.log_level.to_lowercase() == "info" {
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
                find_game_interface_ip().unwrap_or(std::net::Ipv4Addr::new(127, 0, 0, 1))
            }
        }
    } else {
        match find_game_interface_ip() {
            Some(ip) => {
                inject_system_message(
                    app,
                    SystemLogLevel::Info,
                    "Sniffer",
                    format!("Auto-Targeting Network Interface: {}", ip),
                );
                emit_sniffer_state(
                    app,
                    "Binding",
                    &format!("Auto-Targeting Network Interface: {}", ip),
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
        emit_sniffer_state(app, "Error", &msg);
        return Err(msg);
    }

    // 3. Enable Promiscuous Mode safely
    let rcval: u32 = 1;
    let mut out_buffer = [0u8; 4];
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
        "Loopback",
        "vEthernet",
        "TAP",
        "Tailscale",
        "WireGuard",
        "OpenVPN",
        "Radmin",
        "Hamachi",
        "ZeroTier",
        "VMware",
        "VirtualBox",
        "WSL",
        "Npcap",
    ];

    for (name, ip) in network_interfaces {
        let name_lower = name.to_lowercase();

        // Skip if the adapter name contains any of the blocked keywords
        if ignore_list
            .iter()
            .any(|&keyword| name_lower.contains(&keyword.to_lowercase()))
        {
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
            inject_system_message(
                app,
                SystemLogLevel::Info,
                "Sniffer",
                "Configuring Windows Firewall...",
            );
            emit_sniffer_state(app, "Firewall", "Configuring Windows Firewall...");

            let _ = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "delete",
                    "rule",
                    &format!("name={}", RULE_NAME),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            let result = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    &format!("name={}", RULE_NAME),
                    "dir=in",
                    "action=allow",
                    "protocol=TCP",
                    "remoteport=5003",
                    "remoteip=172.65.0.0/16",
                    &format!("program={}", path_str),
                    "enable=yes",
                    "profile=any",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            match result {
                Ok(status) if status.success() => {
                    inject_system_message(
                        app,
                        SystemLogLevel::Success,
                        "Sniffer",
                        "Firewall configured successfully.",
                    );
                }
                _ => {
                    inject_system_message(
                        app,
                        SystemLogLevel::Error,
                        "Sniffer",
                        "Failed to configure firewall. Inbound chat may be blocked.",
                    );
                }
            }
        }
    }
}

pub fn remove_firewall_rule() {
    let _ = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={}", RULE_NAME),
        ])
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

#[tauri::command]
pub fn ensure_firewall_rule_command(app: tauri::AppHandle) -> Result<String, String> {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(path_str) = exe_path.to_str() {
            inject_system_message(
                &app,
                SystemLogLevel::Info,
                "Sniffer",
                "Requesting Administrator privileges to configure Windows Firewall...",
            );

            // Delete old rule first
            let _ = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "delete",
                    "rule",
                    &format!("name={}", RULE_NAME),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            // Create new rule
            let result = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    &format!("name={}", RULE_NAME),
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

pub fn check_firewall_rule() -> bool {
    let result = Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule", &format!("name={}", RULE_NAME)])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match result {
        Ok(output) => output.status.success(), // Returns true if the rule exists
        Err(_) => false,
    }
}