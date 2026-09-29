use crate::capture::message_processor::{MessageProcessor, ProcessAction};
use crate::capture::stream_tracker::StreamTracker;
use crate::protocol::parser::{parsing_pipeline, Port5003Event};
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

    /// 100% Pure Logic: Takes raw network bytes and returns UI Actions.
    pub fn feed_network_packet(
        &mut self,
        packet: &[u8],
        blocked_users: &HashMap<u64, String>,
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

        // 2. Build the unique TCP stream key
        let mut stream_key = [0u8; 6];
        stream_key[0..4].copy_from_slice(&ipv4.source);
        stream_key[4..6].copy_from_slice(&tcp.source_port.to_be_bytes());

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

                // 5. Apply duplicate and blocking rules
                match self.processor.process(&mut chat, blocked_users) {
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
        let blocked_users = HashMap::new();

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
            &blocked_users,
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
        let blocked = HashMap::new();
        let mut pid = 0;
        let mut out = Vec::new();
        for seg in segments {
            let actions = pipeline.feed_network_packet(
                &tcp_from_5003(seg),
                &blocked,
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
