use crate::protocol::framing::FrameAssembler;
use std::collections::HashMap;

/// One frame assembler per TCP stream (server address + port).
pub struct StreamTracker {
    streams: HashMap<[u8; 6], FrameAssembler>,
}

impl StreamTracker {
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
        }
    }

    /// Takes raw bytes from a specific connection and returns fully assembled packets
    pub fn process_bytes(&mut self, stream_key: [u8; 6], payload: &[u8]) -> Vec<Vec<u8>> {
        self.streams.entry(stream_key).or_default().push(payload)
    }
}
