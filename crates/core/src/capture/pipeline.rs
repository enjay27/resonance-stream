use crate::capture::message_processor::{MessageProcessor, ProcessAction};
use crate::capture::stream_tracker::{StreamKey, StreamTracker};
use crate::protocol::parser::{parsing_pipeline, Port5003Event};
use crate::text::normalize_emotes;
use etherparse::{NetHeaders, PacketHeaders, TransportHeader};
use resonance_types::ChatMessage;
use std::collections::HashMap;

pub enum PipelineAction {
    UpdateBlockedMessage(ChatMessage),
    EmitNewMessage(ChatMessage),
}

pub struct ChatPipeline {
    tracker: StreamTracker,
    processor: MessageProcessor,
    keep_unknown_fields: bool,
}

impl ChatPipeline {
    pub fn new() -> Self {
        Self {
            tracker: StreamTracker::new(),
            processor: MessageProcessor::new(),
            keep_unknown_fields: false,
        }
    }

    /// Keep the raw bytes of fields the parser does not understand on each
    /// message (a reverse-engineering aid). Off by default: they are never
    /// displayed, and would otherwise be cloned, stored and sent to the UI.
    pub fn set_keep_unknown_fields(&mut self, keep: bool) {
        self.keep_unknown_fields = keep;
    }

    /// Teaches the duplicate check messages shown before (e.g. history
    /// reloaded from disk), so the server re-sending them is ignored.
    pub fn remember(&mut self, messages: &[ChatMessage]) {
        for message in messages {
            self.processor.commit_new_message(message);
        }
    }

    /// 100% Pure Logic: Takes raw network bytes and returns UI Actions.
    pub fn feed_network_packet(
        &mut self,
        packet: &[u8],
        is_blocked: impl Fn(u64) -> bool,
        mut assign_pid: impl FnMut() -> u64,
        mut feed_watchdog: impl FnMut(),
    ) -> Vec<PipelineAction> {
        let mut actions = Vec::new();

        // 1. Guard clauses: Fail fast if it's not the exact IPv4/TCP/5003 packet we want
        let Ok(headers) = PacketHeaders::from_ip_slice(packet) else {
            return actions;
        };
        let Some(TransportHeader::Tcp(tcp)) = headers.transport else {
            return actions;
        };
        if tcp.source_port != 5003 {
            return actions;
        }

        let payload = headers.payload.slice();
        if payload.is_empty() {
            return actions;
        }

        feed_watchdog();

        let Some(NetHeaders::Ipv4(ipv4, _)) = headers.net else {
            return actions;
        };

        // 2. Build the unique TCP connection key (both ends)
        let mut stream_key: StreamKey = [0u8; 12];
        stream_key[0..4].copy_from_slice(&ipv4.source);
        stream_key[4..6].copy_from_slice(&tcp.source_port.to_be_bytes());
        stream_key[6..10].copy_from_slice(&ipv4.destination);
        stream_key[10..12].copy_from_slice(&tcp.destination_port.to_be_bytes());

        // 3. Assemble fragmented bytes into complete Protobuf packets
        let assembled_packets = self.tracker.process_bytes(stream_key, payload);

        // 4. Process the fully assembled packets
        for packet_data in assembled_packets {
            for event in parsing_pipeline(&packet_data) {
                // Guard clause for the event loop using let-else
                let Port5003Event::Chat(mut chat) = event;
                if !self.keep_unknown_fields {
                    chat.unknown_fields = HashMap::new();
                }
                // Display form of stickers/emotes, before anything (the
                // translator included) sees the text.
                chat.message = normalize_emotes(&chat.message);

                // 5. Apply duplicate and blocking rules
                match self.processor.process(&mut chat, &is_blocked) {
                    ProcessAction::IgnoreDuplicate => continue,
                    ProcessAction::UpdateBlockedMessage => {
                        actions.push(PipelineAction::UpdateBlockedMessage(chat));
                    }
                    ProcessAction::EmitNewMessage => {
                        chat.pid = assign_pid();
                        self.processor.commit_new_message(&chat);
                        actions.push(PipelineAction::EmitNewMessage(chat));
                    }
                }
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use etherparse::PacketBuilder;
    use std::collections::HashMap;

    #[test]
    fn test_full_chat_pipeline() {
        let mut pipeline = ChatPipeline::new();
        let blocked_users: HashMap<u64, String> = HashMap::new();

        let mut mock_pid_counter = 1;
        let mut assign_pid = || {
            let pid = mock_pid_counter;
            mock_pid_counter += 1;
            pid
        };

        // 1. Construct the raw Protobuf payload for a complete BPSR Chat Message
        // Includes Session ID (Sequence ID), Sender Info (Nickname & UID), and Message Text
        let tcp_payload = vec![
            0x00, 0x00, 0x00, 0x00, // Fake 4-byte application header
            0x0A, // Protobuf Root Tag
            0x14, // Root Length: 20 bytes
            // --- ChatPayload Wrapper (Tag 18 -> 0x12) ---
            0x12, 0x12, // Tag 18, Length 18
            // --- Field 1: Session ID (Tag 8 -> 0x08) ---
            0x08, 0xE7, 0x07, // Sequence ID = 999
            // --- Field 2: SenderInfo (Tag 18 -> 0x12) ---
            0x12, 0x07, // Tag 18, Length 7
            0x08, 0x64, // UID (Tag 8) = 100
            0x12, 0x03, 0x42, 0x6F, 0x62, // Nickname (Tag 18) = "Bob"
            // --- Field 4: Message (Tag 34 -> 0x22) ---
            0x22, 0x04, // Tag 34, Length 4
            0x1A, 0x02, 0x48, 0x69, // Text (Tag 26) = "Hi"
        ];

        // 2. Wrap it in valid IPv4 and TCP headers
        let builder =
            PacketBuilder::ipv4([192, 168, 1, 1], [192, 168, 1, 2], 64).tcp(5003, 12345, 1, 0);

        let mut fake_network_packet = Vec::<u8>::new();
        builder
            .write(&mut fake_network_packet, &tcp_payload)
            .unwrap();

        // 3. Feed the forged packet into our pure pipeline
        let actions = pipeline.feed_network_packet(
            &fake_network_packet,
            |uid| blocked_users.contains_key(&uid),
            &mut assign_pid,
            || {}, // Pass an empty closure for the test!
        );

        // 4. Assert that the pipeline correctly extracted ALL fields!
        assert_eq!(actions.len(), 1);
        if let PipelineAction::EmitNewMessage(chat) = &actions[0] {
            assert_eq!(chat.pid, 1, "Local UI PID should be 1");
            assert_eq!(
                chat.sequence_id, 999,
                "Sequence ID from packet should be 999"
            );
            assert_eq!(chat.nickname, "Bob", "Nickname should be Bob");
            assert_eq!(chat.uid, 100, "UID should be 100");
            assert_eq!(chat.message, "Hi", "Message should be Hi");
        } else {
            panic!("Pipeline failed to emit a new message.");
        }
    }
    fn tcp_from_5003(payload: &[u8]) -> Vec<u8> {
        let builder =
            PacketBuilder::ipv4([192, 168, 1, 1], [192, 168, 1, 2], 64).tcp(5003, 12345, 1, 0);
        let mut out = Vec::new();
        builder.write(&mut out, payload).unwrap();
        out
    }

    /// Like `tcp_from_5003`, but to a chosen client (address and port).
    fn tcp_from_5003_to(client: [u8; 4], port: u16, payload: &[u8]) -> Vec<u8> {
        let builder = PacketBuilder::ipv4([192, 168, 1, 1], client, 64).tcp(5003, port, 1, 0);
        let mut out = Vec::new();
        builder.write(&mut out, payload).unwrap();
        out
    }

    #[test]
    fn two_clients_on_one_server_are_reassembled_independently() {
        // W1: the stream key was server address + port only, so the halves of
        // two clients' split frames were joined into one buffer and lost.
        let mut pipeline = ChatPipeline::new();
        let hello = chat_segment(1, "Hello");
        let world = chat_segment(2, "World");
        let (a1, a2) = hello.split_at(10);
        let (b1, b2) = world.split_at(10);
        let order = [
            ([10, 0, 0, 1], 40000, a1),
            ([10, 0, 0, 2], 40001, b1),
            ([10, 0, 0, 1], 40000, a2),
            ([10, 0, 0, 2], 40001, b2),
        ];
        let mut got = Vec::new();
        for (client, port, part) in order {
            for action in pipeline.feed_network_packet(
                &tcp_from_5003_to(client, port, part),
                |_| false,
                || 1,
                || {},
            ) {
                if let PipelineAction::EmitNewMessage(chat) = action {
                    got.push(chat.message);
                }
            }
        }
        assert_eq!(got, ["Hello", "World"]);
    }

    /// App header + root { field 4: { tag 0x1A text } } -- a "Me" message.
    fn me_segment(text: &str) -> Vec<u8> {
        let mut block = vec![0x1A, text.len() as u8];
        block.extend_from_slice(text.as_bytes());
        let mut root = vec![0x22, block.len() as u8];
        root.extend(block);
        let mut seg = vec![0, 0, 0, 0, 0x0A, root.len() as u8];
        seg.extend(root);
        seg
    }

    fn emitted(pipeline: &mut ChatPipeline, segments: &[Vec<u8>]) -> Vec<ChatMessage> {
        let blocked: HashMap<u64, String> = HashMap::new();
        let mut pid = 0;
        let mut out = Vec::new();
        for seg in segments {
            let actions = pipeline.feed_network_packet(
                &tcp_from_5003(seg),
                |uid| blocked.contains_key(&uid),
                || {
                    pid += 1;
                    pid
                },
                || {},
            );
            for action in actions {
                if let PipelineAction::EmitNewMessage(chat) = action {
                    out.push(chat);
                }
            }
        }
        out
    }

    #[test]
    fn every_me_message_is_emitted() {
        let mut pipeline = ChatPipeline::new();
        let got = emitted(&mut pipeline, &[me_segment("one"), me_segment("two")]);
        let texts: Vec<_> = got.iter().map(|c| c.message.as_str()).collect();
        assert_eq!(texts, ["one", "two"]);
    }

    /// App header + root { field 2: chat { seq, sender "Bob", message } }.
    fn chat_segment(seq: u8, text: &str) -> Vec<u8> {
        let mut msg = vec![0x1A, text.len() as u8];
        msg.extend_from_slice(text.as_bytes());
        let sender = [0x08, 0x64, 0x12, 0x03, b'B', b'o', b'b'];
        let mut payload = vec![0x08, seq, 0x12, sender.len() as u8];
        payload.extend_from_slice(&sender);
        payload.extend([0x22, msg.len() as u8]);
        payload.extend(msg);
        let mut root = vec![0x12, payload.len() as u8];
        root.extend(payload);
        let mut seg = vec![0, 0, 0, 0, 0x0A, root.len() as u8];
        seg.extend(root);
        seg
    }

    fn texts(chats: &[ChatMessage]) -> Vec<&str> {
        chats.iter().map(|c| c.message.as_str()).collect()
    }

    #[test]
    fn coalesced_messages_are_all_emitted() {
        // Regression (review B2): only the last message of a segment survived.
        let mut pipeline = ChatPipeline::new();
        let segment = [chat_segment(1, "Hello"), chat_segment(2, "World")].concat();
        let got = emitted(&mut pipeline, &[segment]);
        assert_eq!(texts(&got), ["Hello", "World"]);
    }

    #[test]
    fn message_split_across_segments_is_reassembled() {
        // Regression (review B2): the first half never ended on a boundary and was dropped.
        let mut pipeline = ChatPipeline::new();
        let whole = chat_segment(1, "Hello");
        let (a, b) = whole.split_at(10);
        let got = emitted(&mut pipeline, &[a.to_vec(), b.to_vec()]);
        assert_eq!(texts(&got), ["Hello"]);
    }

    #[test]
    fn split_and_coalesced_together() {
        let mut pipeline = ChatPipeline::new();
        let stream = [
            chat_segment(1, "one"),
            chat_segment(2, "two"),
            chat_segment(3, "three"),
        ]
        .concat();
        let (a, rest) = stream.split_at(12);
        let (b, c) = rest.split_at(30);
        let got = emitted(&mut pipeline, &[a.to_vec(), b.to_vec(), c.to_vec()]);
        assert_eq!(texts(&got), ["one", "two", "three"]);
    }

    #[test]
    fn emotes_are_normalized_before_leaving_the_pipeline() {
        let mut pipeline = ChatPipeline::new();
        let got = emitted(
            &mut pipeline,
            &[
                chat_segment(1, "hi<sprite=3>"),
                chat_segment(2, "emojiPic=9"),
            ],
        );
        assert_eq!(texts(&got), ["hi[이모지]", "[스티커]"]);
    }

    #[test]
    fn blocklist_is_consulted_only_for_chat_messages() {
        use std::cell::Cell;
        let mut pipeline = ChatPipeline::new();
        let lookups = Cell::new(0);
        let is_blocked = |uid: u64| {
            lookups.set(lookups.get() + 1);
            uid == 100
        };

        // Other traffic: not port 5003, and a 5003 segment that is not chat.
        let other = PacketBuilder::ipv4([1, 1, 1, 1], [2, 2, 2, 2], 64).tcp(443, 5000, 1, 0);
        let mut https = Vec::new();
        other.write(&mut https, b"hello").unwrap();
        pipeline.feed_network_packet(&https, &is_blocked, || 1, || {});
        pipeline.feed_network_packet(&tcp_from_5003(&[1, 2, 3, 4, 5]), &is_blocked, || 1, || {});
        assert_eq!(lookups.get(), 0);

        // One chat message from uid 100: one lookup, and it is marked blocked.
        let actions = pipeline.feed_network_packet(
            &tcp_from_5003(&chat_segment(1, "hi")),
            &is_blocked,
            || 1,
            || {},
        );
        assert_eq!(lookups.get(), 1);
        match &actions[..] {
            [PipelineAction::EmitNewMessage(chat)] => assert!(chat.is_blocked),
            _ => panic!("expected one new message"),
        }
    }

    #[test]
    fn remembered_messages_are_not_emitted_again() {
        let mut pipeline = ChatPipeline::new();
        // What a previous run saw and saved: seq 1 "Hello" from uid 100.
        let first = emitted(&mut pipeline, &[chat_segment(1, "Hello")]);
        let mut fresh = ChatPipeline::new();
        fresh.remember(&first);
        let again = emitted(
            &mut fresh,
            &[chat_segment(1, "Hello"), chat_segment(2, "New")],
        );
        assert_eq!(texts(&again), ["New"]);
    }

    /// App header + a chat whose payload carries an unknown field (tag 0x28 = field 5).
    fn chat_with_unknown_field() -> Vec<u8> {
        let payload = [
            0x08, 0x01, // session id 1
            0x12, 0x02, 0x08, 0x64, // sender uid 100
            0x28, 0x07, // unknown field 5 = 7
            0x22, 0x04, 0x1A, 0x02, b'H', b'i', // message "Hi"
        ];
        let mut root = vec![0x12, payload.len() as u8];
        root.extend_from_slice(&payload);
        let mut seg = vec![0, 0, 0, 0, 0x0A, root.len() as u8];
        seg.extend(root);
        seg
    }

    #[test]
    fn unknown_fields_are_dropped_unless_asked_for() {
        let mut pipeline = ChatPipeline::new();
        let got = emitted(&mut pipeline, &[chat_with_unknown_field()]);
        assert_eq!(got.len(), 1);
        assert!(got[0].unknown_fields.is_empty());

        let mut pipeline = ChatPipeline::new();
        pipeline.set_keep_unknown_fields(true);
        let got = emitted(&mut pipeline, &[chat_with_unknown_field()]);
        assert!(!got[0].unknown_fields.is_empty());
    }
}
