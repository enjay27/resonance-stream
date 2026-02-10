use pnet::datalink::{self, Channel::Ethernet};
use pnet::packet::ethernet::EthernetPacket;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::tcp::TcpPacket;
use pnet::packet::Packet;
use pnet::ipnetwork::IpNetwork; // Import this to check IPs safely
use std::sync::mpsc::Sender;
use std::thread;

const GAME_PORT: u16 = 51000;

pub fn start_sniffer(tx: Sender<String>) {
    thread::spawn(move || {
        let interfaces = datalink::interfaces();

        println!("--- [Sniffer] Scanning Network Interfaces ---");
        for iface in &interfaces {
            println!("Found: {} | MAC: {:?} | IPs: {:?}",
                     iface.name, iface.mac, iface.ips);
        }
        println!("---------------------------------------------");

        // --- NEW LOGIC: FIND BY IP ---
        // We look for ANY interface that has a valid IPv4 address (not 0.0.0.0 and not localhost).
        // We ignore "is_up()" because Windows sometimes reports false for working adapters.
        let interface = interfaces
            .into_iter()
            .find(|iface| {
                iface.ips.iter().any(|ip| {
                    match ip {
                        IpNetwork::V4(v4) => {
                            let s = v4.ip().to_string();
                            s != "0.0.0.0" && s != "127.0.0.1"
                        },
                        _ => false
                    }
                })
            })
            // Fallback: If that fails, match the GUID directly from your logs
            .or_else(|| {
                println!("[Sniffer] IP Filter failed. Trying manual GUID match...");
                datalink::interfaces().into_iter().find(|iface|
                    iface.name.contains("4DC99CBD-4CAA-40F1-8F0F-9555859FFEAF")
                )
            });

        let interface = match interface {
            Some(iface) => iface,
            None => {
                eprintln!("[Sniffer] ERROR: No valid network interface found! Packet capture is disabled.");
                return;
            }
        };

        println!("[Sniffer] Selected Interface: {} ({:?})", interface.name, interface.description);

        let (_, mut rx) = match datalink::channel(&interface, Default::default()) {
            Ok(Ethernet(tx, rx)) => (tx, rx),
            Ok(_) => {
                eprintln!("[Sniffer] Error: Unhandled channel type");
                return;
            },
            Err(e) => {
                eprintln!("[Sniffer] Error creating channel: {}", e);
                return;
            }
        };

        loop {
            match rx.next() {
                Ok(packet) => {
                    process_packet(packet, &tx);
                }
                Err(e) => {
                    eprintln!("[Sniffer] Read Error: {}", e);
                }
            }
        }
    });
}

fn process_packet(ethernet_bytes: &[u8], tx: &Sender<String>) {
    if let Some(ethernet) = EthernetPacket::new(ethernet_bytes) {
        if let Some(ipv4) = Ipv4Packet::new(ethernet.payload()) {
            if let Some(tcp) = TcpPacket::new(ipv4.payload()) {
                if tcp.get_source() == GAME_PORT || tcp.get_destination() == GAME_PORT {
                    let payload = tcp.payload();
                    if payload.is_empty() { return; }

                    if let Ok(text) = std::str::from_utf8(payload) {
                        let clean_text = text.trim();
                        if clean_text.len() > 1 && clean_text.chars().all(|c| !c.is_control()) {
                            let json_msg = serde_json::json!({ "text": clean_text }).to_string();
                            let _ = tx.send(json_msg);
                        }
                    }
                }
            }
        }
    }
}