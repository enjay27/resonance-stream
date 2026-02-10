// src-tauri/src/sniffer.rs
use std::sync::mpsc::Sender;
use std::thread;
use windivert::prelude::*;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::tcp::TcpPacket;
use pnet::packet::Packet;
use serde::Serialize;

#[derive(Serialize)]
pub struct ChatPacket {
    #[serde(rename = "UID")]
    pub uid: u64,
    pub nickname: String,
    pub channel: String,
    pub timestamp: i64,
    pub message: String,
}

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
            if let Ok(packet) = wd.recv(Some(&mut buffer)) {
                let payload = get_tcp_payload(&packet.data); // Helper to strip TCP headers

                if payload.len() > 22 && payload[4..6] == [0, 2] { // Opcode 2
                    let proto_data = &payload[22..];

                    if let Some(chat_data) = parse_protobuf_chat(proto_data) {
                        // Filter: Only send to Python if it's Japanese
                        if is_japanese(&chat_data.message) {
                            if let Ok(json) = serde_json::to_string(&chat_data) {
                                tx.send(json).ok();
                            }
                        }
                    }
                }
            }
        }
    });
}

// Robust Protobuf Parser for Message (Tag 26) and Channel (Tag 32)
fn parse_protobuf_chat(data: &[u8]) -> Option<ChatPacket> {
    let mut nickname = String::new();
    let mut message = String::new();
    let mut channel = "WORLD".to_string();
    let mut uid: u64 = 0;

    // Iterate through bytes to find Tags
    let mut i = 0;
    while i < data.len().saturating_sub(1) {
        match data[i] {
            0x12 => { // Nickname Tag (inside nested user block)
                // This is simplified; ideally use a proper proto-reader
                // But for now, we look for the ASCII/UTF-8 name
            },
            0x20 => { // Tag 32 (Channel)
                channel = match data[i+1] {
                    1 => "AREA".into(),
                    2 => "WORLD".into(),
                    3 => "PARTY".into(),
                    4 => "GUILD".into(),
                    _ => "WORLD".into(),
                };
            },
            0x1A => { // Tag 26 (The Chat String)
                let (len, bytes_read) = read_varint(&data[i+1..]);
                let start = i + 1 + bytes_read;
                let end = start + len;
                if end <= data.len() {
                    message = String::from_utf8_lossy(&data[start..end]).into_owned();
                }
            },
            _ => {}
        }
        i += 1;
    }

    if message.is_empty() || message.starts_with("emojiPic") { return None; }

    Some(ChatPacket {
        uid: 0, // Extracting the nested UID requires a full proto-parser
        nickname: "DetectedPlayer".into(), // You can refine name extraction similarly
        channel,
        timestamp: 1739234334, // Replace with actual current time
        message,
    })
}

// Varint Reader for Protobuf Lengths
fn read_varint(data: &[u8]) -> (usize, usize) {
    let mut value: usize = 0;
    let mut bytes_read = 0;
    for &byte in data {
        value |= ((byte & 0x7F) as usize) << (7 * bytes_read);
        bytes_read += 1;
        if (byte & 0x80) == 0 { break; }
    }
    (value, bytes_read)
}

fn is_japanese(text: &str) -> bool {
    text.chars().any(|c|
                         (c >= '\u{3040}' && c <= '\u{309F}') || // Hiragana
                             (c >= '\u{30A0}' && c <= '\u{30FF}') || // Katakana
                             (c >= '\u{4E00}' && c <= '\u{9FAF}')    // Kanji
    )
}

fn get_tcp_payload(data: &[u8]) -> &[u8] {
    // 1. Ensure we have enough data for an IPv4 header (min 20 bytes)
    if data.len() < 20 { return &[]; }

    // 2. Get IPv4 Header Length (IHL)
    // The lower 4 bits of the first byte multiplied by 4 gives the offset
    let ihl = (data[0] & 0x0F) as usize * 4;

    // 3. Ensure we have enough data for the TCP header
    let tcp_start = ihl;
    if data.len() < tcp_start + 20 { return &[]; }

    // 4. Get TCP Data Offset
    // The high 4 bits of the 13th byte (index 12) of the TCP header
    // multiplied by 4 gives the offset to the payload
    let data_offset_byte = data[tcp_start + 12];
    let tcp_header_len = ((data_offset_byte >> 4) as usize) * 4;

    let payload_start = tcp_start + tcp_header_len;

    // 5. Return the slice of the original buffer
    if data.len() > payload_start {
        &data[payload_start..]
    } else {
        &[]
    }
}