// src-tauri/src/sniffer.rs
use std::sync::mpsc::Sender;
use std::thread;
use windivert::prelude::*;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::tcp::TcpPacket;
use pnet::packet::Packet;

pub fn start_sniffer(tx: Sender<String>) {
    thread::spawn(move || {
        println!("--- [Eye] SNIFFER ACTIVE ON PORT 5003 ---");

        // Filter strictly for the Chat Server
        let filter = "tcp.PayloadLength > 0 and (tcp.SrcPort == 5003 or tcp.DstPort == 5003)";
        let flags = WinDivertFlags::new().set_sniff();

        let wd = match WinDivert::network(filter, 0, flags) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[Sniffer] FATAL ERROR: {:?}", e);
                return;
            }
        };

        let mut buffer = [0u8; 65535];

        loop {
            match wd.recv(Some(&mut buffer)) {
                Ok(packet) => {
                    if let Some(ipv4) = Ipv4Packet::new(&packet.data) {
                        if let Some(tcp) = TcpPacket::new(ipv4.payload()) {
                            let payload = tcp.payload();

                            // Check if the payload is large enough to contain our header
                            if payload.len() >= 6 {
                                // 1. Read Length (First 4 bytes, Big Endian)
                                let mut len_bytes = [0u8; 4];
                                len_bytes.copy_from_slice(&payload[0..4]);
                                let packet_length = u32::from_be_bytes(len_bytes) as usize;

                                // 2. Read Opcode (Next 2 bytes, Big Endian)
                                let mut opcode_bytes = [0u8; 2];
                                opcode_bytes.copy_from_slice(&payload[4..6]);
                                let opcode = u16::from_be_bytes(opcode_bytes);

                                // 3. Is it a Chat Packet? (Opcode 2)
                                // We also ensure the payload actually matches the stated length to avoid fragmented garbage
                                if opcode == 2 && payload.len() >= packet_length {

                                    // 4. Extract the Japanese Text
                                    // We skip the 22-byte header/routing info and read the Protobuf body
                                    println!("[Original] Payload: {:?}", &payload);
                                    if let Some(text) = extract_japanese_text(&payload[22..packet_length]) {
                                        println!("[Sniffer] Extracted: {}", text);

                                        // Send to the Tauri frontend -> Python Sidecar -> Qwen!
                                        if let Err(e) = tx.send(text) {
                                            eprintln!("[Sniffer] Failed to send to UI: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        }
    });
}

/// A robust byte-scanner that pulls readable UTF-8 strings out of raw Protobuf data.
fn extract_japanese_text(data: &[u8]) -> Option<String> {
    // Convert the raw bytes into a lossy string
    let raw_string = String::from_utf8_lossy(data);

    // Filter out control characters and the '' replacement character
    let clean_text: String = raw_string
        .chars()
        .filter(|c| !c.is_control() && *c != '\u{FFFD}')
        .collect();

    let trimmed = clean_text.trim();

    // Ensure we actually caught something substantial before returning it
    if trimmed.len() > 1 {
        Some(trimmed.to_string())
    } else {
        None
    }
}