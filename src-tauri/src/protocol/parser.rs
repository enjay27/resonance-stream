use crate::protocol::decoder::{find_int_by_tag, find_string_by_tag, read_varint, skip_field};
pub(crate) use crate::ChatMessage;
use crate::{inject_system_message, SystemLogLevel};
use std::collections::HashMap;
use tauri::AppHandle;

#[derive(Debug)]
pub enum Port5003Event {
    Chat(ChatMessage),
}

#[derive(Debug)]
pub struct SplitPayload<'a> {
    pub channel: String,
    pub chat_blocks: Vec<(u32, &'a [u8])>,
}

#[derive(Debug, Default)]
pub struct ChatPayload {
    pub session_id: u64,    // Tag 8 (Field 1): 20
    pub sender: SenderInfo, // Tag 18 (Field 2): Player info block
    pub timestamp: u64,     // Tag 24 (Field 3): 1772343736
    pub message: String,    // Tag 34 (Field 4): Message string block
    pub unknown_fields: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct SenderInfo {
    pub uid: u64,         // Tag 8 (Field 1): 37276266
    pub nickname: String, // Tag 18 (Field 2): "あずるる"
    pub class_id: u64,    // Tag 24 (Field 3): 2 (e.g., Twin Striker)
    pub status: u64,      // Tag 32 (Field 4): 1 (Online/Normal flag)
    pub level: u64,       // Tag 40 (Field 5): 60
    pub is_blocked: bool,
    pub unknown_fields: HashMap<String, Vec<u8>>,
}

pub fn parsing_pipeline(data: &[u8]) -> Vec<Port5003Event> {
    let raw_payload = match stage1_split(data) {
        Some(p) => p,
        None => return Vec::new(),
    };

    // Use standard log::trace! instead of Tauri's inject_system_message
    log::trace!("[5003] stage 1 completed {:?}", raw_payload);

    let events = stage2_process(raw_payload);

    log::trace!("[5003] stage 2 completed {:?}", events);

    events
}

// --- STAGE 1: SPLIT ---
// Separates the raw Protobuf packet into categorized byte blocks.
pub(crate) fn stage1_split(data: &[u8]) -> Option<SplitPayload> {
    let mut payload = SplitPayload {
        channel: "WORLD".to_string(),
        chat_blocks: Vec::new(),
    };

    if data.len() < 3 || data[0] != 0x0A {
        return None;
    }

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
                    }
                    _ => {}
                }
            }
            i = block_end;
        } else if wire_type == 0 {
            let (val, read) = read_varint(&data[i..safe_end]);

            if field_num == 1 || field_num == 2 {
                payload.channel = match val {
                    2 => "LOCAL".into(),
                    3 => "PARTY".into(),
                    4 => "GUILD".into(),
                    _ => "WORLD".into(),
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
        let mut chat = ChatMessage {
            channel: raw.channel.clone(),
            ..Default::default()
        };

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

                chat.unknown_fields = parsed_payload.unknown_fields;
                chat.unknown_fields
                    .extend(parsed_payload.sender.unknown_fields);
            }
            4 => {
                if let Some(msg) = find_string_by_tag(block, 0x1A) {
                    chat.message = msg;
                    if let Some(chan_id) = find_int_by_tag(block, 0x10) {
                        chat.channel = match chan_id {
                            3 => "PARTY".into(),
                            4 => "GUILD".into(),
                            _ => chat.channel,
                        };
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
        i += 1;

        match tag {
            8 => {
                // Session ID / Sequence ID
                let (val, read) = read_varint(&data[i..]);
                payload.session_id = val;
                i += read;
            }
            18 => {
                // SenderInfo Block
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    payload.sender = parse_sender_info(sub_data);
                }
                i = block_end;
            }
            24 => {
                // Timestamp
                let (val, read) = read_varint(&data[i..]);
                payload.timestamp = val;
                i += read;
            }
            34 => {
                // Message Block -> Delegated to extracted function!
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    parse_message_block(sub_data, &mut payload);
                }
                i = block_end;
            }
            _ => {
                // Skip Unknown
                let skipped = skip_field(wire_type, &data[i..]);
                let safe_end = (i + skipped).min(data.len());
                payload
                    .unknown_fields
                    .insert(format!("chat_{}", tag), data[i..safe_end].to_vec());
                i += skipped;
            }
        }
    }
    payload
}

// A clean, dedicated function just for handling rich text arrays!
fn parse_rich_content(data: &[u8], unknown_fields: &mut HashMap<String, Vec<u8>>) -> String {
    let mut parsed_text = String::new();
    let mut k = 0;

    while k < data.len() {
        let r_tag = data[k];
        let r_wire = r_tag & 0x07;
        k += 1;

        if r_tag == 18 {
            // Chunk Block (Field 2)
            let (clen, cr) = read_varint(&data[k..]);
            k += cr;
            let c_end = (k + clen as usize).min(data.len());

            // Delegate to ANOTHER small function!
            parsed_text.push_str(&parse_chunk_block(&data[k..c_end], unknown_fields));

            k = c_end;
        } else {
            let skipped = skip_field(r_wire, &data[k..]);
            let safe_end = (k + skipped).min(data.len());
            unknown_fields.insert(format!("rich_{}", r_tag), data[k..safe_end].to_vec());
            k += skipped;
        }
    }
    parsed_text
}

// Handles the specific Chunk Type (Text vs Item Link vs Fish)
fn parse_chunk_block(chunk: &[u8], unknown_fields: &mut HashMap<String, Vec<u8>>) -> String {
    let mut chunk_type = 0;
    let mut chunk_text = String::new();
    let mut l = 0;

    while l < chunk.len() {
        let c_tag = chunk[l];
        let c_wire = c_tag & 0x07;
        l += 1;

        if c_tag == 8 {
            // Chunk Type
            let (val, vr) = read_varint(&chunk[l..]);
            chunk_type = val;
            l += vr;
        } else if c_tag == 18 {
            // Chunk Payload
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
            let skipped = skip_field(c_wire, &chunk[l..]);
            let safe_end = (l + skipped).min(chunk.len());
            unknown_fields.insert(format!("chunk_{}", c_tag), chunk[l..safe_end].to_vec());
            l += skipped;
        }
    }

    match chunk_type {
        7 => chunk_text, // Text Chunk
        3 => "[아이템 링크]".to_string(),
        2 => "[개인 공간]".to_string(),
        9 => "[물고기 자랑]".to_string(),
        12 => "[마스터 점수]".to_string(),
        _ => "".to_string(),
    }
}

fn parse_sender_info(data: &[u8]) -> SenderInfo {
    let mut sender = SenderInfo::default();
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let wire_type = tag & 0x07;
        i += 1;

        match tag {
            8 => {
                // Tag 8 = Field 1 (UID)
                let (val, read) = read_varint(&data[i..]);
                sender.uid = val;
                i += read;
            }
            18 => {
                // Tag 18 = Field 2 (Nickname)
                let (len, read) = read_varint(&data[i..]);
                i += read;
                let block_end = (i + len as usize).min(data.len());
                if let Some(sub_data) = data.get(i..block_end) {
                    sender.nickname = String::from_utf8_lossy(sub_data).into_owned();
                }
                i = block_end;
            }
            32 => {
                // Tag 32 = Field 4 (Status Flag)
                let (val, read) = read_varint(&data[i..]);
                sender.status = val;
                i += read;
            }
            40 => {
                // Tag 40 = Field 5 (Level)
                let (val, read) = read_varint(&data[i..]);
                sender.level = val;
                i += read;
            }
            // Tags 24 (Platform?), 56 (Rank?), and 64 (Badge?)
            // will now safely fall into the skip_field wildcard!
            _ => {
                let skipped = skip_field(wire_type, &data[i..]);
                let safe_end = (i + skipped).min(data.len());
                sender
                    .unknown_fields
                    .insert(format!("sender_{}", tag), data[i..safe_end].to_vec());
                i += skipped;
            }
        }
    }
    sender
}

fn parse_message_block(data: &[u8], payload: &mut ChatPayload) {
    let mut j = 0;

    while j < data.len() {
        let sub_tag = data[j];
        let sub_wire = sub_tag & 0x07;
        j += 1;

        match sub_tag {
            26 => {
                // Normal Chat Text
                let (slen, r) = read_varint(&data[j..]);
                j += r;
                let s_end = (j + slen as usize).min(data.len());
                if j < s_end {
                    let text_msg = String::from_utf8_lossy(&data[j..s_end]).into_owned();
                    payload.message.push_str(&text_msg);
                }
                j = s_end;
            }
            58 => {
                // Rich Content Array (Item Links, Fishing, etc.)
                let (rlen, rr) = read_varint(&data[j..]);
                j += rr;
                let r_end = (j + rlen as usize).min(data.len());
                let rich_data = &data[j..r_end];

                let rich_text = parse_rich_content(rich_data, &mut payload.unknown_fields);
                payload.message.push_str(&rich_text);

                // [CRITICAL FIX]: Advance the pointer past the block so it doesn't parse garbage!
                j = r_end;
            }
            _ => {
                // Safely skip unknown inner tags
                let skipped = skip_field(sub_wire, &data[j..]);
                let safe_end = (j + skipped).min(data.len());
                payload
                    .unknown_fields
                    .insert(format!("msg_{}", sub_tag), data[j..safe_end].to_vec());
                j += skipped;
            }
        }
    }
}

// ==========================================
// PARSING & UTILITIES (Keep your existing functions below)
// ==========================================
pub(crate) fn strip_application_header(payload: &[u8], port: u16) -> Option<&[u8]> {
    if payload.len() < 5 {
        return None;
    }

    match port {
        10250 => {
            if payload.len() > 32 && payload[32] == 0x0A {
                Some(&payload[32..])
            } else {
                None
            }
        }
        5003 => {
            // Search for the 0x0A that correctly describes the rest of the payload
            for i in 0..payload.len().saturating_sub(3) {
                if payload[i] == 0x0A {
                    let (msg_len, varint_size) = read_varint(&payload[i + 1..]);
                    // If this 0x0A + its length exactly matches the end of the TCP packet, it's real
                    if varint_size > 0 && (i + 1 + varint_size + msg_len as usize) == payload.len()
                    {
                        return Some(&payload[i..]);
                    }
                }
            }
            None
        }
        _ => {
            if payload[0] == 0x0A {
                Some(payload)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_read_varint() {
        // 300 in Varint is 10101100 00000010 (0xAC 0x02)
        let data = [0xAC, 0x02, 0xFF];
        let (val, read_bytes) = read_varint(&data);
        assert_eq!(val, 300);
        assert_eq!(read_bytes, 2);
    }

    #[test]
    fn test_skip_field() {
        // Wire Type 0 (Varint)
        let data_varint = [0xAC, 0x02, 0xFF];
        assert_eq!(skip_field(0, &data_varint), 2);

        // Wire Type 1 (64-bit / 8 bytes)
        let data_64bit = [0; 10];
        assert_eq!(skip_field(1, &data_64bit), 8);

        // Wire Type 2 (Length-delimited)
        // Length 3, followed by 3 bytes = total 4 bytes to skip
        let data_length = [0x03, 0xAA, 0xBB, 0xCC, 0xFF];
        assert_eq!(skip_field(2, &data_length), 4);
    }

    #[test]
    fn test_strip_application_header_5003() {
        // Fake TCP Packet from port 5003
        // Includes garbage app header [0x00, 0x00, 0x11, 0x22]
        // Real payload starts at 0x0A, len 0x02, payload [0xBB, 0xCC]
        let packet = [0x00, 0x00, 0x11, 0x22, 0x0A, 0x02, 0xBB, 0xCC];

        let stripped = strip_application_header(&packet, 5003).unwrap();

        // Should perfectly ignore the first 4 bytes
        assert_eq!(stripped, &[0x0A, 0x02, 0xBB, 0xCC]);

        // Test rejection of bad packet
        let bad_packet = [0x00, 0x11, 0x22, 0x0A, 0x09, 0xBB]; // Claims length 9, but ends early
        assert!(strip_application_header(&bad_packet, 5003).is_none());
    }

    #[test]
    fn test_parser_edge_cases() {
        // Edge Case 1: strip_application_header with tiny payloads
        let tiny_payload = [0x0A, 0x01]; // Length is only 2 bytes
        assert!(strip_application_header(&tiny_payload, 5003).is_none());
        assert!(strip_application_header(&tiny_payload, 10250).is_none());

        // Edge Case 2: Truncated Varint parsing
        // The byte 0xAC indicates continuation, but the buffer ends abruptly!
        let truncated_data = [0xAC];
        let (val, read_bytes) = read_varint(&truncated_data);

        // UPDATED: Our new safe decoder correctly identifies this as incomplete
        // and returns 0 bytes read to signal "Wait for more data".
        assert_eq!(read_bytes, 0);
        assert_eq!(val, 0);

        // Edge Case 3: skip_field with out-of-bounds length
        // Wire type 2 (length-delimited). The byte 0x32 decodes to length 50,
        // but the buffer only has 2 bytes left after it.
        let out_of_bounds_data = [0x32, 0xFF, 0xFF];
        let skipped = skip_field(2, &out_of_bounds_data);

        // skip_field correctly parses the varint (1 byte) and adds the requested length (50).
        assert_eq!(skipped, 51);

        // Let's manually verify the caller's safety net
        let safe_end = (0 + skipped).min(out_of_bounds_data.len());
        assert_eq!(safe_end, 3); // Capped safely at buffer length!
    }

    #[test]
    fn test_parse_chunk_block() {
        let mut unknown_fields = HashMap::new();

        // 1. Test Item Link (Chunk Type 3)
        // Tag 8 (0x08) -> Value 3 (0x03)
        let item_chunk = [0x08, 0x03];
        assert_eq!(
            parse_chunk_block(&item_chunk, &mut unknown_fields),
            "[아이템 링크]"
        );

        // 2. Test Fish Record (Chunk Type 9)
        // Tag 8 (0x08) -> Value 9 (0x09)
        let fish_chunk = [0x08, 0x09];
        assert_eq!(
            parse_chunk_block(&fish_chunk, &mut unknown_fields),
            "[물고기 자랑]"
        );

        // 3. Test Text Chunk (Chunk Type 7) with nested string
        // This simulates: Type = 7, Payload = { Tag 10 = "Hello" }
        let text_chunk = [
            0x08, 0x07, // Tag 8 (Type), Value 7
            0x12, 0x07, // Tag 18 (Payload), Length 7
            0x0A, 0x05, // Tag 10 (String), Length 5
            b'H', b'e', b'l', b'l', b'o',
        ];
        assert_eq!(parse_chunk_block(&text_chunk, &mut unknown_fields), "Hello");
    }

    #[test]
    fn test_parse_rich_content_array() {
        let mut unknown_fields = HashMap::new();

        // This array simulates two consecutive Rich Content elements wrapped in Tag 18 (0x12):
        // 1. A Text chunk saying "Look at this: "
        // 2. An Item chunk
        let rich_data = [
            // --- First Element: Text Chunk ---
            0x12, 0x14, // Array Wrapper: Tag 18, Length 20
            0x08, 0x07, // Chunk Type: 7
            0x12, 0x10, // Chunk Payload: Tag 18, Length 16
            0x0A, 0x0E, // String: Tag 10, Length 14
            b'L', b'o', b'o', b'k', b' ', b'a', b't', b' ', b't', b'h', b'i', b's', b':', b' ',
            // --- Second Element: Item Chunk ---
            0x12, 0x02, // Array Wrapper: Tag 18, Length 2
            0x08, 0x03, // Chunk Type: 3
        ];

        let result = parse_rich_content(&rich_data, &mut unknown_fields);

        // It should perfectly stitch the text and the mapped item link together!
        assert_eq!(result, "Look at this: [아이템 링크]");
    }

    #[test]
    fn test_parse_message_block() {
        let mut payload = ChatPayload::default();

        // Simulate the inner bytes of Tag 34 (Message Block)
        // It contains a Normal Text block (Tag 26) followed by a Rich Content block (Tag 58)
        let message_data = [
            // --- Tag 26: Normal Text ---
            26, 6, // Tag 26, Length 6
            b'H', b'e', b'l', b'l', b'o', b' ',
            // --- Tag 58: Rich Content (Item Link) ---
            58, 4, // Tag 58, Length 4
            18, 2, // Array Wrapper: Tag 18, Length 2
            8, 3, // Chunk Type: 3 (Item Link)
            // --- Tag 26: Normal Text Again ---
            26, 1, // Tag 26, Length 1
            b'!',
        ];

        // Process the block
        parse_message_block(&message_data, &mut payload);

        // It should perfectly stitch all 3 pieces together in order!
        assert_eq!(payload.message, "Hello [아이템 링크]!");
        // Ensure no garbage fell into unknown_fields because of bad pointer math
        assert!(payload.unknown_fields.is_empty());
    }

    #[test]
    fn test_parse_chat_payload_flattened() {
        // Construct the full ChatPayload byte array (what resides inside the outer Tag 18)
        let chat_payload_data = vec![
            // 1. Session ID (Tag 8 -> 0x08)
            8, 0xE7, 0x07, // Value: 999
            // 2. Sender Info (Tag 18 -> 0x12)
            18, 7, // Tag 18, Length 7
            8, 100, // UID: 100
            18, 3, b'B', b'o', b'b', // Nickname: "Bob"
            // 3. Timestamp (Tag 24 -> 0x18)
            24, 0x80, 0x01, // Value: 128
            // 4. Message Block (Tag 34 -> 0x22)
            34, 7, // Tag 34, Length 7
            26, 5, b'G', b'r', b'e', b'a', b't', // Tag 26, Length 5, "Great"
        ];

        let parsed = parse_chat_payload(&chat_payload_data);

        // Verify the top-level fields
        assert_eq!(parsed.session_id, 999);
        assert_eq!(parsed.timestamp, 128);
        assert_eq!(parsed.message, "Great");

        // Verify the nested Sender Info was delegated correctly
        assert_eq!(parsed.sender.uid, 100);
        assert_eq!(parsed.sender.nickname, "Bob");
    }
}
