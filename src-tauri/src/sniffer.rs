use std::collections::HashMap;
// src-tauri/src/sniffer.rs
use std::sync::mpsc::Sender;
use std::thread;
use windivert::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};
use crate::packet_buffer::PacketBuffer;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ChatPacket {
    pub channel: String,      // e.g., "PARTY", "LOCAL"
    pub entity_id: u64,       // e.g., 80
    pub uid: u64,             // e.g., 823656
    pub nickname: String,     // e.g., "NAME"
    pub class_id: u64,        // e.g., 2
    pub status_flag: u64,     // e.g., 1
    pub level: u64,           // e.g., 60
    pub timestamp: u64,       // e.g., 1770753503
    pub message: String,      // e.g., "hi" or "emojiPic=..."
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

        // Map of [IP+Port] -> PacketBuffer
        let mut streams: HashMap<[u8; 6], PacketBuffer> = HashMap::new();
        let mut buffer = [0u8; 65535];

        loop {
            if let Ok(packet) = wd.recv(Some(&mut buffer)) {
                let raw_data = packet.data;

                // 1. Get the Stream Key using the RAW data (so we know the IP/Port)
                if let Some(stream_key) = extract_stream_key(&*raw_data) {

                    // 2. EXTRACT THE PAYLOAD (Strip IP & TCP Headers)
                    if let Some(payload) = extract_tcp_payload(&*raw_data) {

                        // CRITICAL FIX: Skip empty TCP ACKs.
                        // A payload length of 0 means it's just a network ping, no game data.
                        if payload.is_empty() {
                            continue;
                        }

                        // 3. Now we feed ONLY the game data into the buffer
                        let p_buf = streams.entry(stream_key).or_insert_with(PacketBuffer::new);
                        p_buf.add(payload);

                        // 4. Try to drain full packets based on your 2-byte header logic
                        while let Some(full_packet) = p_buf.next() {
                            println!("full packet: {:?}", full_packet);

                            // Send to parser (skipping the 2-byte length header)
                            if let Some(chat) = parse_star_resonance(&full_packet) {
                                println!("chat: {:?}", chat);
                                window.emit("new-chat-message", &chat).unwrap();
                            }
                        }
                    }
                }
            }
        }
    });
}

// ==========================================
// PROTOBUF PARSING (DEEP EXTRACTION)
// ==========================================

pub fn parse_star_resonance(data: &[u8]) -> Option<ChatPacket> {
    // 1. PacketBuffer guarantees data starts with 0x0A.
    // If it doesn't, this isn't a valid chat packet.
    if data.is_empty() || data[0] != 0x0A {
        return None;
    }

    // 2. No offsets needed! Just use the data directly.
    let stream = data;

    let mut chat = ChatPacket::default();
    let mut i = 1; // Skip the 0x0A tag

    let (total_len, read) = read_varint(&stream[i..]);
    i += read;

    let safe_end = (i + total_len as usize).min(stream.len());

    // --- ROOT LEVEL SCAN ---
    while i < safe_end {
        // ... (Keep your existing match loop exactly the same)
        let tag = stream[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // Channel ID
                let (val, read) = read_varint(&stream[i..safe_end]);
                chat.channel = match val {
                    2 => "LOCAL".into(),
                    3 => "PARTY".into(),
                    4 => "GUILD".into(),
                    _ => "WORLD".into(),
                };
                i += read;
            }
            2 => { // User Container Block
                let (len, read) = read_varint(&stream[i..safe_end]);
                i += read;
                let block_end = (i + len as usize).min(safe_end);

                if let Some(sub_data) = stream.get(i..block_end) {
                    parse_user_container(sub_data, &mut chat);
                }
                i = block_end;
            }
            _ => i += skip_field(wire_type, &stream[i..safe_end]),
        }
    }
    println!("chat inside: {:?}", chat);

    if !chat.message.is_empty() && chat.uid > 0 { Some(chat) } else { None }
}

fn parse_user_container(data: &[u8], chat: &mut ChatPacket) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // Entity ID (Session ID)
                let (val, read) = read_varint(&data[i..]);
                chat.entity_id = val;
                i += read;
            }
            2 => { // Profile Block
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());

                if let Some(sub_data) = data.get(i..block_end) {
                    parse_profile_block(sub_data, chat);
                }
                i = block_end;
            }
            3 => { // Timestamp
                let (val, read) = read_varint(&data[i..]);
                chat.timestamp = val;
                i += read;
            }
            4 => { // Message Block
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());

                if let Some(sub_data) = data.get(i..block_end) {
                    // Chat string is always Tag 3 (0x1A) inside the Message Block
                    if let Some(msg) = find_string_by_tag(sub_data, 0x1A) {
                        chat.message = msg;
                    }
                }
                i = block_end;
            }
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
}

fn parse_profile_block(data: &[u8], chat: &mut ChatPacket) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = tag >> 3;
        i += 1;

        match field_num {
            1 => { // Permanent UID
                let (val, read) = read_varint(&data[i..]);
                chat.uid = val;
                i += read;
            }
            2 => { // Nickname
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());

                if let Some(sub_data) = data.get(i..block_end) {
                    chat.nickname = String::from_utf8_lossy(sub_data).into_owned();
                }
                i = block_end;
            }
            3 => { // Class ID
                let (val, read) = read_varint(&data[i..]);
                chat.class_id = val;
                i += read;
            }
            4 => { // Status Flag
                let (val, read) = read_varint(&data[i..]);
                chat.status_flag = val;
                i += read;
            }
            5 => { // Level
                let (val, read) = read_varint(&data[i..]);
                chat.level = val;
                i += read;
            }
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
}

// --- UTILITIES ---

fn find_string_by_tag(data: &[u8], target_tag: u8) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        if tag == target_tag {
            let (len, read) = read_varint(&data[i+1..]);
            let start = i + 1 + read;
            let end = (start + len as usize).min(data.len());
            if start < end {
                return Some(String::from_utf8_lossy(&data[start..end]).into_owned());
            }
        }
        let wire_type = tag & 0x07;
        i += 1 + skip_field(wire_type, &data[i+1..]);
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

fn extract_stream_key(data: &[u8]) -> Option<[u8; 6]> {
    if data.len() < 20 || (data[0] >> 4) != 4 || data[9] != 6 { return None; } // Must be IPv4 + TCP
    let ihl = (data[0] & 0x0F) as usize * 4;
    if data.len() < ihl + 20 { return None; }

    let mut key = [0u8; 6];
    key[0..4].copy_from_slice(&data[12..16]); // Source IP
    key[4..6].copy_from_slice(&data[ihl..ihl + 2]); // Source Port
    Some(key)
}

fn extract_tcp_payload(data: &[u8]) -> Option<&[u8]> {
    // Basic IPv4 check
    if data.len() < 20 || (data[0] >> 4) != 4 || data[9] != 6 { return None; }

    // IP Header Length (usually 20 bytes, but can be more)
    let ip_header_len = (data[0] & 0x0F) as usize * 4;
    if data.len() < ip_header_len + 20 { return None; }

    // TCP Header Length (Offset is at byte 12 of the TCP header)
    let tcp_header_len = ((data[ip_header_len + 12] >> 4) as usize) * 4;

    // The actual game data starts after both headers
    let payload_offset = ip_header_len + tcp_header_len;

    if payload_offset <= data.len() {
        Some(&data[payload_offset..])
    } else {
        None
    }
}