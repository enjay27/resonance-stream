use crate::packet_buffer::read_varint_safe;
use crate::{inject_system_message, SystemLogLevel};
pub(crate) use crate::{ChatMessage};
use tauri::AppHandle;

#[derive(Debug)]
pub struct SplitPayload<'a> {
    pub channel: String,
    pub chat_blocks: Vec<(u32, &'a [u8])>,
}

#[derive(Debug, Default)]
pub struct ChatPayload {
    pub session_id: u64,        // Tag 8 (Field 1): 20
    pub sender: SenderInfo,     // Tag 18 (Field 2): Player info block
    pub timestamp: u64,         // Tag 24 (Field 3): 1772343736
    pub message: String,        // Tag 34 (Field 4): Message string block
}

#[derive(Debug, Default)]
pub struct SenderInfo {
    pub uid: u64,             // Tag 8 (Field 1): 37276266
    pub nickname: String,     // Tag 18 (Field 2): "あずるる"
    pub class_id: u64,        // Tag 24 (Field 3): 2 (e.g., Twin Striker)
    pub status: u64,          // Tag 32 (Field 4): 1 (Online/Normal flag)
    pub level: u64,           // Tag 40 (Field 5): 60
    pub is_blocked: bool,
}

pub fn parsing_pipeline(data: &[u8], app: &AppHandle) -> Vec<Port5003Event>{
    let raw_payload = match stage1_split(data) {
        Some(p) => p,
        None => return Vec::new(),
    };

    inject_system_message(app, SystemLogLevel::Trace, "Sniffer", format!("[5003] stage 1 completed {:?}", raw_payload));

    let events = stage2_process(raw_payload);

    inject_system_message(app, SystemLogLevel::Trace, "Sniffer", format!("[5003] stage 2 completed {:?}", events));

    events
}

// --- STAGE 1: SPLIT ---
// Separates the raw Protobuf packet into categorized byte blocks.
pub(crate) fn stage1_split<'a>(data: &'a [u8]) -> Option<SplitPayload<'a>> {
    let mut payload = SplitPayload {
        channel: "WORLD".to_string(),
        chat_blocks: Vec::new(),
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
        i += 1;

        if wire_type == 2 {
            let (len, read) = read_varint(&data[i..safe_end]);
            i += read;
            let block_end = (i + len as usize).min(safe_end);

            if let Some(sub_data) = data.get(i..block_end) {
                match field_num {
                    2 | 4 => {
                        payload.chat_blocks.push((field_num, sub_data));
                        is_valid_chat_packet = true;
                    },
                    _ => {}
                }
            }
            i = block_end;
        } else if wire_type == 0 {
            let (val, read) = read_varint(&data[i..safe_end]);

            if field_num == 1 || field_num == 2 {
                payload.channel = match val {
                    2 => "LOCAL".into(), 3 => "PARTY".into(), 4 => "GUILD".into(), _ => "WORLD".into(),
                };
            }
            i += read;
        } else {
            i += skip_field(wire_type, &data[i..safe_end]);
        }
    }

    if is_valid_chat_packet {
        Some(payload)
    } else {
        None
    }
}

// --- STAGE 2: PROCESS ---
// Applies strict, field-mapped parsing logic to generate specific Events.
pub(crate) fn stage2_process(raw: SplitPayload<'_>) -> Vec<Port5003Event> {
    let mut events = Vec::new();

    // 1. Process Chat Blocks
    for (field_num, block) in raw.chat_blocks {
        let mut chat = ChatMessage { channel: raw.channel.clone(), ..Default::default() };

        match field_num {
            2 => {
                // block is now exactly a &[u8], parsing effortlessly
                let parsed_payload = parse_chat_payload(block);

                chat.sequence_id = parsed_payload.session_id;
                chat.timestamp = parsed_payload.timestamp;
                chat.message = parsed_payload.message;

                chat.uid = parsed_payload.sender.uid;
                chat.nickname = parsed_payload.sender.nickname;
                chat.class_id = parsed_payload.sender.class_id;
                chat.level = parsed_payload.sender.level;
                chat.is_blocked = parsed_payload.sender.is_blocked;
            }
            4 => {
                if let Some(msg) = find_string_by_tag(block, 0x1A) {
                    chat.message = msg;
                    if let Some(chan_id) = find_int_by_tag(block, 0x10) {
                        chat.channel = match chan_id { 3 => "PARTY".into(), 4 => "GUILD".into(), _ => chat.channel };
                    }
                }
            }
            _ => {}
        }

        if !chat.message.is_empty() {
            if chat.uid == 0 && chat.nickname.is_empty() {
                chat.nickname = "Me".to_string();
            }

            if !chat.is_blocked {
                events.push(Port5003Event::Chat(chat));
            }
        }
    }

    events
}

// --- STRICT MAPPED PARSERS ---

fn parse_chat_payload(data: &[u8]) -> ChatPayload {
    let mut payload = ChatPayload::default();
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        i += 1; // Advance past the tag

        match tag {
            8 => { // Tag 8 = Field 1, Wire 0 (Session ID / Sequence ID)
                let (val, read) = read_varint(&data[i..]);
                payload.session_id = val;
                i += read;
            }
            18 => { // Tag 18 = Field 2, Wire 2 (SenderInfo Block)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    payload.sender = parse_sender_info(sub_data);
                }
                i = block_end;
            }
            24 => { // Tag 24 = Field 3, Wire 0 (Timestamp)
                let (val, read) = read_varint(&data[i..]);
                payload.timestamp = val;
                i += read;
            }
            34 => { // Tag 34 = Field 4, Wire 2 (Message Block)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());

                if let Some(sub_data) = data.get(i..block_end) {
                    // Parse ALL fields inside the message block
                    let mut j = 0;
                    while j < sub_data.len() {
                        let sub_tag = sub_data[j];
                        let sub_wire = sub_tag & 0x07;
                        j += 1;

                        match sub_tag {
                            26 => { // Normal Chat Text (Field 3)
                                let (slen, r) = read_varint(&sub_data[j..]);
                                j += r;
                                let s_end = (j + slen as usize).min(sub_data.len());
                                if j < s_end {
                                    let text_msg = String::from_utf8_lossy(&sub_data[j..s_end]).into_owned();
                                    payload.message.push_str(&text_msg);
                                }
                                j = s_end;
                            }
                            58 => { // Rich Content Array (Field 7) - Used for Item Links, Personal Space, & Fishing!
                                let (rlen, rr) = read_varint(&sub_data[j..]);
                                j += rr;
                                let r_end = (j + rlen as usize).min(sub_data.len());
                                let rich_data = &sub_data[j..r_end];

                                let mut k = 0;
                                while k < rich_data.len() {
                                    let r_tag = rich_data[k];
                                    let r_wire = r_tag & 0x07;
                                    k += 1;

                                    if r_tag == 18 { // Chunk Block (Field 2)
                                        let (clen, cr) = read_varint(&rich_data[k..]);
                                        k += cr;
                                        let c_end = (k + clen as usize).min(rich_data.len());
                                        let chunk = &rich_data[k..c_end];

                                        let mut chunk_type = 0;
                                        let mut chunk_text = String::new();

                                        let mut l = 0;
                                        while l < chunk.len() {
                                            let c_tag = chunk[l];
                                            let c_wire = c_tag & 0x07;
                                            l += 1;

                                            if c_tag == 8 { // Chunk Type
                                                let (val, vr) = read_varint(&chunk[l..]);
                                                chunk_type = val;
                                                l += vr;
                                            } else if c_tag == 18 { // Chunk Payload
                                                let (plen, pr) = read_varint(&chunk[l..]);
                                                l += pr;
                                                let p_end = (l + plen as usize).min(chunk.len());

                                                // Type 7 = Text Chunk. We must dig one layer deeper to Tag 10 for the string!
                                                if chunk_type == 7 {
                                                    if let Some(txt) = find_string_by_tag(&chunk[l..p_end], 10) {
                                                        chunk_text = txt;
                                                    }
                                                }
                                                l = p_end;
                                            } else {
                                                l += skip_field(c_wire, &chunk[l..]);
                                            }
                                        }

                                        // Append the parsed chunk to the final message!
                                        if chunk_type == 7 {
                                            payload.message.push_str(&chunk_text);
                                        } else if chunk_type == 3 {
                                            payload.message.push_str("[아이템 링크]");
                                        } else if chunk_type == 2 {
                                            payload.message.push_str("[개인 공간]");
                                        } else if chunk_type == 9 {
                                            payload.message.push_str("[물고기 자랑]");
                                        } else if chunk_type == 12 {
                                            payload.message.push_str("[마스터 점수]");
                                        }

                                        k = c_end;
                                    } else {
                                        k += skip_field(r_wire, &rich_data[k..]);
                                    }
                                }
                                j = r_end;
                            }
                            _ => { // Safely skip Tag 8 (Msg Type) and anything else
                                j += skip_field(sub_wire, &sub_data[j..]);
                            }
                        }
                    }
                }
                i = block_end;
            }
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
    payload
}

fn parse_sender_info(data: &[u8]) -> SenderInfo {
    let mut sender = SenderInfo::default();
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        i += 1;

        match tag {
            8 => { // Tag 8 = Field 1, Wire 0 (Permanent UID)
                let (val, read) = read_varint(&data[i..]);
                sender.uid = val;
                i += read;
            }
            18 => { // Tag 18 = Field 2, Wire 2 (Nickname)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    sender.nickname = String::from_utf8_lossy(sub_data).into_owned();
                }
                i = block_end;
            }
            24 => { // Tag 24 = Field 3, Wire 0 (Class ID)
                let (val, read) = read_varint(&data[i..]);
                sender.class_id = val;
                i += read;
            }
            32 => { // Tag 32 = Field 4, Wire 0 (Status Flag)
                let (val, read) = read_varint(&data[i..]);
                sender.status = val;
                i += read;
            }
            40 => { // Tag 40 = Field 5, Wire 0 (Level)
                let (val, read) = read_varint(&data[i..]);
                sender.level = val;
                i += read;
            }
            64 => { // Tag 64 = Field 8, Wire 0 (Blocked Flag)
                let (val, read) = read_varint(&data[i..]);
                sender.is_blocked = val == 1; // Convert integer flag to boolean safely
                i += read;
            }
            _ => i += skip_field(wire_type, &data[i..]),
        }
    }
    sender
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
            64 => { // Tag 64 = Field 8, Wire 0 (Blocked User Flag)
                let (val, read) = read_varint(&data[i..]);
                if val == 1 {
                    chat.is_blocked = true;
                }
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
