// src-tauri/src/sniffer.rs
use std::sync::mpsc::Sender;
use std::thread;
use windivert::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ChatPacket {
    #[serde(rename = "UID")]
    pub uid: u64,
    pub nickname: String,
    pub channel: String,
    pub timestamp: i64,
    pub message: String,
}

pub fn start_sniffer(window: Window) {
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
                // WinDivert handles the IP/TCP headers differently.
                // We need to find the start of the actual Data (Payload)
                let payload = packet.data;

                println!("payload: {:?}", payload);
                if let Some(chat) = parse_star_resonance(&*payload) {
                    window.emit("new-chat-message", &chat).unwrap();
                }
            }
        }
    });
}

// --- TAG-BASED PARSER (ZERO-OFFSET) ---

fn parse_star_resonance(data: &[u8]) -> Option<ChatPacket> {
    // Search for the 0x0A marker (Tag 1, Wire Type 2)
    let start = data.windows(1).position(|w| w[0] == 0x0A)?;
    let stream = &data[start..];

    let mut chat = ChatPacket::default();
    let mut i = 1;

    // Read outer length (Varint)
    let (total_len, read) = read_varint(&stream[i..]);
    i += read;

    while i < stream.len() && i < (total_len as usize + 5) {
        let tag = stream[i];
        let field_num = tag >> 3;
        let wire_type = tag & 0x07;
        i += 1;

        match field_num {
            2 => { // User Sub-block
                let (len, read) = read_varint(&stream[i..]);
                i += read;

                // SAFE SLICE
                let end = (i + len as usize).min(stream.len());
                if i < end {
                    let sub_data = &stream[i..end];
                    extract_user_fields(sub_data, &mut chat);
                }
                i = end;
            }
            4 => { // Message Sub-block
                let (len, read) = read_varint(&stream[i..]);
                i += read;

                // SAFE SLICE
                let end = (i + len as usize).min(stream.len());
                if i < end {
                    let sub_data = &stream[i..end];
                    chat.message = find_string_by_tag(sub_data, 0x1A).unwrap_or_default();
                }
                i = end;
            }
            _ => {
                i += skip_field(wire_type, &stream[i..]);
            }
        }
    }
    println!("chat: {:?}", chat);

    if !chat.message.is_empty() && chat.uid > 0 { Some(chat) } else { None }
}

fn extract_user_fields(data: &[u8], chat: &mut ChatPacket) {
    let mut i = 0;
    while i < data.len() {
        let tag = data.get(i).copied().unwrap_or(0);
        if tag == 0 { break; }

        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // UID
                if let Some(slice) = data.get(i..) {
                    let (val, read) = read_varint(slice);
                    if val > chat.uid { chat.uid = val; }
                    i += read;
                }
            }
            2 => { // Nickname Sub-block
                if let Some(slice) = data.get(i..) {
                    let (len, read) = read_varint(slice);
                    i += read;

                    let start = i;
                    let end = (i + len as usize).min(data.len()); // CLAMP

                    if let Some(sub) = data.get(start..end) {
                        if let Some(name) = find_string_by_tag(sub, 0x12) {
                            chat.nickname = name;
                        }
                    }
                    i = end;
                }
            }
            _ => i += 1,
        }
    }
}

// --- UTILITIES ---

fn find_string_by_tag(data: &[u8], target_tag: u8) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        if data[i] == target_tag {
            // Check if we have enough room to even read a varint
            if i + 1 >= data.len() { return None; }

            let (len, read) = read_varint(&data[i+1..]);
            let start = i + 1 + read;
            let end = start + len as usize;

            // SAFETY: Clamp the end index to the data length
            let safe_end = end.min(data.len());

            if start < safe_end {
                let string_bytes = &data[start..safe_end];
                return Some(String::from_utf8_lossy(string_bytes).into_owned());
            }
        }
        i += 1;
    }
    None
}

fn read_varint(data: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    let mut pos = 0;
    while pos < data.len() {
        let byte = data[pos];
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