use crate::protocol::decoder::{
    field_end, find_int_by_tag, find_string_by_tag, read_varint, Fields, Value,
};
use resonance_types::{Channel, ChatMessage};
use std::collections::HashMap;

#[derive(Debug)]
pub enum Port5003Event {
    Chat(ChatMessage),
}

#[derive(Debug)]
pub struct SplitPayload<'a> {
    pub channel: Channel,
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

/// A `0x8003` history root: `{ 3: channel, 5: chat, 5: chat, ... }`, each
/// `5` laid out like a live root's field `2`, newest first. Returned oldest
/// first, so the lines are shown in the order they were said.
pub fn history_pipeline(data: &[u8]) -> Vec<Port5003Event> {
    let Some(raw) = split_history(data) else {
        return Vec::new();
    };
    let mut events = stage2_process(raw);
    events.reverse();
    events
}

fn split_history(data: &[u8]) -> Option<SplitPayload<'_>> {
    if data.len() < 3 || data[0] != 0x0A {
        return None;
    }
    let (total_len, header_read) = read_varint(&data[1..]);
    let body_start = 1 + header_read;
    let safe_end = field_end(body_start, total_len, data.len());

    let mut payload = SplitPayload {
        channel: Channel::World,
        chat_blocks: Vec::new(),
    };
    for field in Fields::new(&data[body_start..safe_end]) {
        match (field.number(), field.value) {
            (3, Value::Varint(code)) => payload.channel = Channel::from_code(code),
            (5, Value::Bytes(block)) => payload.chat_blocks.push((2, block)),
            _ => {}
        }
    }
    (!payload.chat_blocks.is_empty()).then_some(payload)
}

// --- STAGE 1: SPLIT ---
// Separates the raw Protobuf packet into categorized byte blocks.
pub(crate) fn stage1_split(data: &[u8]) -> Option<SplitPayload<'_>> {
    let mut payload = SplitPayload {
        channel: Channel::World,
        chat_blocks: Vec::new(),
    };

    if data.len() < 3 || data[0] != 0x0A {
        return None;
    }

    let (total_len, header_read) = read_varint(&data[1..]);
    let body_start = 1 + header_read;
    let safe_end = field_end(body_start, total_len, data.len());

    let mut is_valid_chat_packet = false;

    for field in Fields::new(&data[body_start..safe_end]) {
        match field.value {
            Value::Bytes(block) => {
                if matches!(field.number(), 2 | 4) {
                    payload.chat_blocks.push((field.number() as u32, block));
                    is_valid_chat_packet = true;
                }
            }
            Value::Varint(val) => {
                if matches!(field.number(), 1 | 2) {
                    payload.channel = Channel::from_code(val);
                }
            }
            Value::Other => {}
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
            channel: raw.channel,
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
                            3 => Channel::Party,
                            4 => Channel::Guild,
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
    for field in Fields::new(data) {
        match (field.tag, field.value) {
            // Session ID / Sequence ID
            (8, Value::Varint(id)) => payload.session_id = id,
            // SenderInfo Block
            (18, Value::Bytes(sender)) => payload.sender = parse_sender_info(sender),
            // Timestamp
            (24, Value::Varint(timestamp)) => payload.timestamp = timestamp,
            // Message Block
            (34, Value::Bytes(message)) => parse_message_block(message, &mut payload),
            _ => {
                payload
                    .unknown_fields
                    .insert(format!("chat_{}", field.tag), field.raw.to_vec());
            }
        }
    }
    payload
}

// A clean, dedicated function just for handling rich text arrays!
fn parse_rich_content(data: &[u8], unknown_fields: &mut HashMap<String, Vec<u8>>) -> String {
    let mut parsed_text = String::new();
    for field in Fields::new(data) {
        match (field.tag, field.value) {
            // Chunk Block (Field 2)
            (18, Value::Bytes(chunk)) => {
                parsed_text.push_str(&parse_chunk_block(chunk, unknown_fields));
            }
            _ => {
                unknown_fields.insert(format!("rich_{}", field.tag), field.raw.to_vec());
            }
        }
    }
    parsed_text
}

// Handles the specific Chunk Type (Text vs Item Link vs Fish)
fn parse_chunk_block(chunk: &[u8], unknown_fields: &mut HashMap<String, Vec<u8>>) -> String {
    let mut chunk_type = 0;
    let mut chunk_text = String::new();
    for field in Fields::new(chunk) {
        match (field.tag, field.value) {
            // Chunk Type
            (8, Value::Varint(kind)) => chunk_type = kind,
            // Chunk Payload. Type 7 = Text Chunk: the string is one layer
            // deeper, at tag 10.
            (18, Value::Bytes(payload)) => {
                if chunk_type == 7 {
                    if let Some(text) = find_string_by_tag(payload, 10) {
                        chunk_text = text;
                    }
                }
            }
            _ => {
                unknown_fields.insert(format!("chunk_{}", field.tag), field.raw.to_vec());
            }
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
    for field in Fields::new(data) {
        match (field.tag, field.value) {
            // Tag 8 = Field 1 (UID)
            (8, Value::Varint(uid)) => sender.uid = uid,
            // Tag 18 = Field 2 (Nickname)
            (18, Value::Bytes(name)) => {
                sender.nickname = String::from_utf8_lossy(name).into_owned();
            }
            // Tag 32 = Field 4 (Status Flag)
            (32, Value::Varint(status)) => sender.status = status,
            // Tag 40 = Field 5 (Level)
            (40, Value::Varint(level)) => sender.level = level,
            // Tags 24 (Platform?), 56 (Rank?), and 64 (Badge?) and the rest
            // are kept as unknown fields.
            _ => {
                sender
                    .unknown_fields
                    .insert(format!("sender_{}", field.tag), field.raw.to_vec());
            }
        }
    }
    sender
}

fn parse_message_block(data: &[u8], payload: &mut ChatPayload) {
    for field in Fields::new(data) {
        match (field.tag, field.value) {
            // Normal Chat Text
            (26, Value::Bytes(text)) => {
                if !text.is_empty() {
                    payload.message.push_str(&String::from_utf8_lossy(text));
                }
            }
            // Rich Content Array (Item Links, Fishing, etc.)
            (58, Value::Bytes(rich)) => {
                let rich_text = parse_rich_content(rich, &mut payload.unknown_fields);
                payload.message.push_str(&rich_text);
            }
            _ => {
                payload
                    .unknown_fields
                    .insert(format!("msg_{}", field.tag), field.raw.to_vec());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::decoder::skip_field;

    fn varint_bytes(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }

    fn field_bytes(tag: u8, body: &[u8]) -> Vec<u8> {
        [vec![tag], varint_bytes(body.len() as u64), body.to_vec()].concat()
    }

    /// `{ 1: id, 2: { 1: uid, 2: name }, 3: time, 4: { 3: text } }`
    fn entry(id: u8, uid: u8, name: &str, time: u8, text: &str) -> Vec<u8> {
        [
            vec![0x08, id],
            field_bytes(
                0x12,
                &[&[0x08, uid][..], &field_bytes(0x12, name.as_bytes())].concat(),
            ),
            vec![0x18, time],
            field_bytes(0x22, &field_bytes(0x1A, text.as_bytes())),
        ]
        .concat()
    }

    fn history_root(channel: u8, entries: &[Vec<u8>]) -> Vec<u8> {
        let mut fields = vec![0x18, channel];
        for e in entries {
            fields.extend(field_bytes(0x2A, e));
        }
        field_bytes(0x0A, &fields)
    }

    fn texts(events: &[Port5003Event]) -> Vec<&str> {
        events
            .iter()
            .map(|Port5003Event::Chat(c)| c.message.as_str())
            .collect()
    }

    #[test]
    fn history_lines_come_out_oldest_first() {
        // The server sends the newest line first.
        let root = history_root(
            1,
            &[
                entry(3, 7, "Cid", 30, "newest"),
                entry(2, 6, "Ann", 20, "middle"),
                entry(1, 5, "Bob", 10, "oldest"),
            ],
        );
        let events = history_pipeline(&root);
        assert_eq!(texts(&events), ["oldest", "middle", "newest"]);
        let Port5003Event::Chat(first) = &events[0];
        assert_eq!(first.nickname, "Bob");
        assert_eq!(first.uid, 5);
        assert_eq!(first.sequence_id, 1);
        assert_eq!(first.timestamp, 10);
        assert_eq!(first.channel, Channel::World);
    }

    #[test]
    fn history_channel_follows_the_roots_channel_field() {
        for (code, name) in [
            (2, Channel::Local),
            (3, Channel::Party),
            (4, Channel::Guild),
            (1, Channel::World),
            (9, Channel::Beginner),
        ] {
            let events = history_pipeline(&history_root(code, &[entry(1, 5, "Bob", 10, "x")]));
            let Port5003Event::Chat(chat) = &events[0];
            assert_eq!(chat.channel, name);
        }
    }

    #[test]
    fn a_history_root_without_lines_or_a_live_root_gives_nothing() {
        assert!(history_pipeline(&history_root(1, &[])).is_empty());
        assert!(history_pipeline(&[]).is_empty());
        // A live chat root is not a history root.
        let live = field_bytes(0x0A, &field_bytes(0x12, &entry(1, 5, "Bob", 10, "x")));
        assert!(history_pipeline(&live).is_empty());
        assert_eq!(texts(&parsing_pipeline(&live)), ["x"]);
    }

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
    fn test_parser_edge_cases() {
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
    /// Wraps `root_fields` in a `0x0A <len>` root, as it arrives after the app header.
    fn root(root_fields: &[u8]) -> Vec<u8> {
        let mut out = vec![0x0A, root_fields.len() as u8];
        out.extend_from_slice(root_fields);
        out
    }

    #[test]
    fn huge_length_varint_does_not_panic() {
        // Regression (review B1): message sub-tag 58 with a u64::MAX length.
        let huge = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];
        let mut msg = vec![58u8];
        msg.extend_from_slice(&huge);
        msg.extend_from_slice(&[0, 0, 0]);
        let mut chat = vec![34u8, msg.len() as u8];
        chat.extend(&msg);
        let mut field2 = vec![0x12u8, chat.len() as u8];
        field2.extend(&chat);
        parsing_pipeline(&root(&field2));

        // The same huge length at every length-delimited position of a chat payload.
        for tag in [18u8, 34, 26, 58, 0x12, 0x0A] {
            let mut inner = vec![tag];
            inner.extend_from_slice(&huge);
            inner.extend_from_slice(&[1, 2, 3]);
            let mut f2 = vec![0x12u8, inner.len() as u8];
            f2.extend(&inner);
            parsing_pipeline(&root(&f2));
            let mut f4 = vec![0x22u8, inner.len() as u8];
            f4.extend(&inner);
            parsing_pipeline(&root(&f4));
        }
    }

    #[test]
    fn random_bytes_never_panic() {
        // Small deterministic xorshift, so the test needs no extra dependency.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..50_000 {
            let len = (next() % 96) as usize;
            let mut data: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            // Bias towards plausible input: a root tag and a chat field up front.
            if len > 2 && next() % 2 == 0 {
                data[0] = 0x0A;
                data[2] = if next() % 2 == 0 { 0x12 } else { 0x22 };
            }
            // Splice in a maximal length varint: random bytes almost never form one.
            if len > 12 && next() % 3 == 0 {
                let at = (next() as usize) % (len - 10);
                data[at..at + 10]
                    .copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
            }
            parsing_pipeline(&data);
            for frame in crate::protocol::framing::FrameAssembler::new().push(&data) {
                parsing_pipeline(&frame.root);
                history_pipeline(&frame.root);
            }
            history_pipeline(&data);
        }
    }

    #[test]
    fn multi_byte_tag_is_skipped_as_one_field() {
        // Unknown field 20 (varint) -> tag 160 encodes as two bytes [0xA0, 0x01].
        let data = [
            0xA0, 0x01, 0x05, // field 20 = 5
            24, 0x80, 0x01, // timestamp = 128
            34, 4, 26, 2, b'H', b'i', // message "Hi"
        ];
        let parsed = parse_chat_payload(&data);
        assert_eq!(parsed.timestamp, 128);
        assert_eq!(parsed.message, "Hi");
    }

    // --- Golden characterization (pins parser output across refactors) ---

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn chance(&mut self, percent: u64) -> bool {
            self.below(100) < percent
        }
    }

    fn varint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }
    fn tag(field: u64, wire: u64) -> Vec<u8> {
        varint(field << 3 | wire)
    }
    fn var_field(field: u64, v: u64) -> Vec<u8> {
        [tag(field, 0), varint(v)].concat()
    }
    fn len_field(field: u64, body: &[u8]) -> Vec<u8> {
        [tag(field, 2), varint(body.len() as u64), body.to_vec()].concat()
    }
    fn text(rng: &mut Rng) -> Vec<u8> {
        const POOL: [&str; 6] = [
            "",
            "Hi",
            "こんにちは",
            "募集 3周 @Ab1",
            "<sprite=3>x",
            "【火力】",
        ];
        if rng.chance(15) {
            (0..rng.below(6)).map(|_| rng.next() as u8).collect()
        } else {
            POOL[rng.below(6) as usize].as_bytes().to_vec()
        }
    }
    fn random_field(rng: &mut Rng) -> Vec<u8> {
        let field = 1 + rng.below(40);
        match rng.below(5) {
            0 => var_field(field, rng.next() >> rng.below(64)),
            1 => len_field(field, &text(rng)),
            2 => [tag(field, 1), (0..8).map(|_| rng.next() as u8).collect()].concat(),
            3 => [tag(field, 5), (0..4).map(|_| rng.next() as u8).collect()].concat(),
            _ => tag(field, 3 + 3 * rng.below(2)), // wire types 3 / 6, no body
        }
    }
    fn chunk(rng: &mut Rng) -> Vec<u8> {
        let kind = [7, 7, 7, 3, 2, 9, 12, 5][rng.below(8) as usize];
        let mut parts = vec![var_field(1, kind)];
        if kind == 7 {
            parts.push(len_field(2, &len_field(1, &text(rng))));
        } else if rng.chance(30) {
            parts.push(len_field(2, &text(rng)));
        }
        if rng.chance(25) {
            parts.push(random_field(rng));
        }
        if rng.chance(15) {
            parts.reverse(); // type after payload
        }
        parts.concat()
    }
    fn message_block(rng: &mut Rng) -> Vec<u8> {
        let mut parts = Vec::new();
        for _ in 0..rng.below(4) {
            parts.push(match rng.below(4) {
                0 | 1 => len_field(3, &text(rng)),
                2 => len_field(
                    7,
                    &(0..1 + rng.below(3))
                        .map(|_| len_field(2, &chunk(rng)))
                        .collect::<Vec<_>>()
                        .concat(),
                ),
                _ => random_field(rng),
            });
        }
        parts.concat()
    }
    fn sender(rng: &mut Rng) -> Vec<u8> {
        let bits = 7 * (1 + rng.below(4));
        let mut parts = vec![var_field(1, rng.below(1 << bits))];
        if rng.chance(80) {
            parts.push(len_field(2, &text(rng)));
        }
        for field in [3, 4, 5, 7, 8] {
            if rng.chance(40) {
                parts.push(var_field(field, rng.below(300)));
            }
        }
        if rng.chance(20) {
            parts.push(random_field(rng));
        }
        parts.concat()
    }
    fn chat_payload(rng: &mut Rng) -> Vec<u8> {
        let mut parts = Vec::new();
        if rng.chance(85) {
            parts.push(var_field(1, rng.below(100_000)));
        }
        if rng.chance(85) {
            parts.push(len_field(2, &sender(rng)));
        }
        if rng.chance(70) {
            parts.push(var_field(3, rng.next() >> 20));
        }
        if rng.chance(90) {
            parts.push(len_field(4, &message_block(rng)));
        }
        for _ in 0..rng.below(3) {
            parts.push(random_field(rng));
        }
        if rng.chance(20) && parts.len() > 1 {
            parts.swap(0, 1);
        }
        parts.concat()
    }
    fn gen_root(rng: &mut Rng) -> Vec<u8> {
        let mut body = Vec::new();
        for _ in 0..1 + rng.below(3) {
            body.extend(match rng.below(6) {
                0..=2 => len_field(2, &chat_payload(rng)),
                3 => len_field(
                    4,
                    &[
                        len_field(3, &text(rng)),
                        if rng.chance(60) {
                            var_field(2, 3 + rng.below(3))
                        } else {
                            vec![]
                        },
                    ]
                    .concat(),
                ),
                4 => var_field(1 + rng.below(2), rng.below(6)),
                _ => random_field(rng),
            });
        }
        let mut out = [vec![0x0A], varint(body.len() as u64), body].concat();
        match rng.below(10) {
            0 => out.truncate(rng.below(out.len() as u64 + 1) as usize),
            1 | 2 => {
                let at = rng.below(out.len() as u64) as usize;
                out[at] = rng.next() as u8;
            }
            3 => {
                let at = rng.below(out.len() as u64) as usize;
                out.splice(at..at, [0xFF; 10]);
            }
            _ => {}
        }
        out
    }

    /// Output in a canonical text form (unknown fields sorted: HashMap order is random).
    fn canonical(events: &[Port5003Event]) -> String {
        let mut out = String::new();
        for Port5003Event::Chat(c) in events {
            let mut unknown: Vec<_> = c.unknown_fields.iter().collect();
            unknown.sort();
            out += &format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}\n",
                c.channel,
                c.nickname,
                c.message,
                c.timestamp,
                c.uid,
                c.class_id,
                c.level,
                c.sequence_id,
                c.is_blocked,
                unknown
            );
        }
        out
    }

    fn fnv1a(hash: &mut u64, bytes: &[u8]) {
        for b in bytes {
            *hash = (*hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01B3);
        }
    }

    #[test]
    fn parser_output_is_pinned_across_refactors() {
        // Characterization: recorded from the hand-written parser loops, so a
        // refactor of them (the field iterator) must not change one byte of output.
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        let (mut hash, mut chats, mut packets) = (0xCBF2_9CE4_8422_2325u64, 0usize, 0usize);
        for _ in 0..40_000 {
            let data = gen_root(&mut rng);
            let events = parsing_pipeline(&data);
            chats += events.len();
            packets += usize::from(!events.is_empty());
            fnv1a(&mut hash, canonical(&events).as_bytes());
            fnv1a(&mut hash, b"\x00");
        }
        assert!(
            chats > 20_000 && packets > 15_000,
            "weak generator: {chats} chats in {packets} packets"
        );
        assert_eq!((hash, chats), (PINNED_HASH, PINNED_CHATS));
    }

    const PINNED_HASH: u64 = 17_370_710_247_901_616_853; // code 9 now reads as BEGINNER, not WORLD
    const PINNED_CHATS: usize = 28_246;
}
