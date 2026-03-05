use std::collections::HashMap;
use crate::packet_buffer::PacketBuffer;

pub struct StreamTracker {
    streams: HashMap<[u8; 6], PacketBuffer>,
}

impl StreamTracker {
    pub fn new() -> Self {
        Self { streams: HashMap::new() }
    }

    /// Takes raw bytes from a specific connection and returns fully assembled packets
    pub fn process_bytes(&mut self, stream_key: [u8; 6], payload: &[u8]) -> Vec<Vec<u8>> {
        let mut assembled_packets = Vec::new();

        // 1. Strip application header (5003)
        if let Some(game_data) = crate::protocol::parser::strip_application_header(payload, 5003) {
            let p_buf = self.streams.entry(stream_key).or_insert_with(PacketBuffer::new);
            p_buf.add(game_data);

            // 2. Extract all complete packets
            while let Some(full_packet) = p_buf.next() {
                assembled_packets.push(full_packet);
            }
        }
        assembled_packets
    }
}