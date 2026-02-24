use crate::packet_buffer::read_varint_safe;
pub(crate) use crate::{AppState, ChatMessage, LobbyRecruitment, ProfileAsset, SplitPayload, SystemMessage};

// --- STAGE 1: SPLIT ---
// Separates the raw Protobuf packet into categorized byte blocks.
pub(crate) fn stage1_split(data: &[u8]) -> Option<SplitPayload> {
    let mut payload = SplitPayload {
        channel: "WORLD".to_string(),
        chat_blocks: Vec::new(),
        recruit_asset_blocks: Vec::new(),
    };

    if data.len() < 3 || data[0] != 0x0A { return None; }

    let (total_len, header_read) = read_varint(&data[1..]);
    let mut i = 1 + header_read;
    let safe_end = (i + total_len as usize).min(data.len());

    let mut is_valid_chat_packet = false;

    while i < safe_end {
        let tag = data[i];
        let wire_type = tag & 0x07;
        let field_num = (tag >> 3) as u32;
        i += 1; // Move past the tag byte

        if wire_type == 2 {
            let (len, read) = read_varint(&data[i..safe_end]);
            i += read;
            let block_end = (i + len as usize).min(safe_end);

            if let Some(sub_data) = data.get(i..block_end) {
                match field_num {
                    2 | 3 | 4 => payload.chat_blocks.push((field_num, sub_data.to_vec())),
                    18 => payload.recruit_asset_blocks.push(sub_data.to_vec()),
                    _ => {}
                }
            }
            i = block_end; // Advance the pointer past this block
        } else if wire_type == 0 {
            let (val, read) = read_varint(&data[i..safe_end]);
            if field_num == 1 {
                // Real Chat/Recruit packets ALWAYS have a Channel ID varint here.
                is_valid_chat_packet = true;
                payload.channel = match val {
                    2 => "LOCAL".into(), 3 => "PARTY".into(), 4 => "GUILD".into(), _ => "WORLD".into(),
                };
            }
            i += read;
        } else {
            // Safely skip any other data types
            i += skip_field(wire_type, &data[i..safe_end]);
        }
    }

    // Filter out Server/Metadata packets immediately
    if is_valid_chat_packet {
        Some(payload)
    } else {
        None
    }
}

// --- STAGE 2: PROCESS ---
// Applies strict, field-mapped parsing logic to generate specific Events.
pub(crate) fn stage2_process(raw: SplitPayload) -> Vec<Port5003Event> {
    let mut events = Vec::new();

    // Context memory for Field 18 (which relies on Field 2 for names/IDs)
    let mut ctx_uid = 0;
    let mut ctx_nickname = String::new();
    let mut ctx_timestamp = 0;

    // 1. Process Chat Blocks
    for (field_num, block) in raw.chat_blocks {
        let mut chat = ChatMessage { channel: raw.channel.clone(), ..Default::default() };

        match field_num {
            2 | 3 => {
                parse_user_container(&block, &mut chat); // Strict parsing
            }
            4 => {
                if let Some(msg) = find_string_by_tag(&block, 0x1A) {
                    chat.message = msg;
                    if let Some(chan_id) = find_int_by_tag(&block, 0x10) {
                        chat.channel = match chan_id { 3 => "PARTY".into(), 4 => "GUILD".into(), _ => chat.channel };
                    }
                }
            }
            _ => {}
        }

        // Save context if we found player identity
        if chat.uid > 0 { ctx_uid = chat.uid; }
        if !chat.nickname.is_empty() { ctx_nickname = chat.nickname.clone(); }
        if chat.timestamp > 0 { ctx_timestamp = chat.timestamp; }

        if !chat.message.is_empty() && chat.uid > 0 {
            events.push(Port5003Event::Chat(chat));
        }
    }

    // 2. Process Recruit & Asset Blocks
    for block in raw.recruit_asset_blocks {
        let text = String::from_utf8_lossy(&block).into_owned();

        if text.contains("ID:") {
            events.push(Port5003Event::Recruit(LobbyRecruitment {
                recruit_id: text.split("ID:").nth(1).unwrap_or("").to_string(),
                leader_nickname: ctx_nickname.clone(),
                description: text,
                timestamp: ctx_timestamp,
                ..Default::default()
            }));
        } else if text.contains("https://") {
            let mut asset = ProfileAsset { uid: ctx_uid, timestamp: ctx_timestamp, ..Default::default() };
            if text.contains("snapshot") { asset.snapshot_url = text; }
            else { asset.halflength_url = text; }
            events.push(Port5003Event::Asset(asset));
        }
    }

    events
}

// ==========================================
// PARSING & UTILITIES (Keep your existing functions below)
// ==========================================
pub(crate) fn strip_application_header(payload: &[u8], port: u16) -> Option<&[u8]> {
    if payload.len() < 5 { return None; }

    match port {
        10250 => {
            if payload.len() > 32 && payload[32] == 0x0A { Some(&payload[32..]) } else { None }
        },
        5003 => {
            // Search for the 0x0A that correctly describes the rest of the payload
            for i in 0..payload.len().saturating_sub(3) {
                if payload[i] == 0x0A {
                    let (msg_len, varint_size) = read_varint_safe(&payload[i+1..]);
                    // If this 0x0A + its length exactly matches the end of the TCP packet, it's real
                    if varint_size > 0 && (i + 1 + varint_size + msg_len as usize) == payload.len() {
                        return Some(&payload[i..]);
                    }
                }
            }
            None
        },
        _ => if payload[0] == 0x0A { Some(payload) } else { None }
    }
}

#[derive(Debug)]
pub enum Port5003Event {
    Chat(ChatMessage),
    Recruit(LobbyRecruitment),
    Asset(ProfileAsset),
}

// --- STRICT MAPPED PARSERS (From previous_sniffer.rs) ---

fn parse_user_container(data: &[u8], chat: &mut ChatMessage) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        i += 1; // Advance past the tag

        // Match on the EXACT tag byte, not just the field number
        match tag {
            8 => { // Tag 8 = Field 1, Wire 0 (Session ID)
                let (val, read) = read_varint(&data[i..]);
                chat.pid = val;
                i += read;
            }
            18 => { // Tag 18 = Field 2, Wire 2 (Profile Block)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    parse_profile_block(sub_data, chat);
                }
                i = block_end;
            }
            24 => { // Tag 24 = Field 3, Wire 0 (Timestamp)
                let (val, read) = read_varint(&data[i..]);
                chat.timestamp = val;
                i += read;
            }
            34 => { // Tag 34 = Field 4, Wire 2 (Message Block)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    if let Some(msg) = find_string_by_tag(sub_data, 0x1A) {
                        chat.message = msg;
                    }
                }
                i = block_end;
            }
            // If we see Tag 32 (Field 4, Wire 0), it safely falls through to skip_field!
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
}

fn parse_profile_block(data: &[u8], chat: &mut ChatMessage) {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        i += 1;

        match tag {
            8 => { // Tag 8 = Field 1, Wire 0 (Permanent UID)
                let (val, read) = read_varint(&data[i..]);
                chat.uid = val;
                i += read;
            }
            18 => { // Tag 18 = Field 2, Wire 2 (Nickname)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    chat.nickname = String::from_utf8_lossy(sub_data).into_owned();
                }
                i = block_end;
            }
            24 => { // Tag 24 = Field 3, Wire 0 (Class ID)
                let (val, read) = read_varint(&data[i..]);
                chat.class_id = val;
                i += read;
            }
            32 => { // Tag 32 = Field 4, Wire 0 (Status Flag)
                let (_, read) = read_varint(&data[i..]);
                i += read;
            }
            40 => { // Tag 40 = Field 5, Wire 0 (Level)
                let (val, read) = read_varint(&data[i..]);
                chat.level = val;
                i += read;
            }
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
}

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

fn find_int_by_tag(data: &[u8], target_tag: u8) -> Option<u64> {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        if tag == target_tag {
            let (val, _) = read_varint(&data[i+1..]);
            return Some(val);
        }
        let wire_type = tag & 0x07;
        i += 1 + skip_field(wire_type, &data[i+1..]);
    }
    None
}

pub(crate) fn read_varint(data: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    let mut pos = 0;
    while pos < data.len() {
        let byte = data[pos];
        if shift >= 64 { return (value, pos); }
        value |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if (byte & 0x80) == 0 { break; }
        shift += 7;
    }
    (value, pos)
}

pub(crate) fn skip_field(wire_type: u8, data: &[u8]) -> usize {
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

pub(crate) fn extract_stream_key(data: &[u8]) -> Option<[u8; 6]> {
    if data.len() < 20 || (data[0] >> 4) != 4 || data[9] != 6 { return None; } // Must be IPv4 + TCP
    let ihl = (data[0] & 0x0F) as usize * 4;
    if data.len() < ihl + 20 { return None; }

    let mut key = [0u8; 6];
    key[0..4].copy_from_slice(&data[12..16]); // Source IP
    key[4..6].copy_from_slice(&data[ihl..ihl + 2]); // Source Port
    Some(key)
}

pub(crate) fn extract_tcp_payload(data: &[u8]) -> Option<&[u8]> {
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