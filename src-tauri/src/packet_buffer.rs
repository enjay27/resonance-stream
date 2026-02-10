// src-tauri/src/packet_buffer.rs
use byteorder::{ByteOrder, LittleEndian};

pub struct PacketBuffer {
    buffer: Vec<u8>,
}

impl PacketBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
        }
    }

    /// Adds raw network bytes to the buffer
    pub fn add(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Tries to extract the next valid game packet from the buffer.
    /// Returns Some(packet_bytes) if a full packet is ready.
    /// Returns None if we are waiting for more data.
    pub fn next(&mut self) -> Option<Vec<u8>> {
        // 1. We need at least 2 bytes to read the Length Header
        if self.buffer.len() < 2 {
            return None;
        }

        // 2. Read the Total Length (First 2 bytes, Little Endian)
        // Blue Protocol headers usually start with [Length: u16]
        let packet_len = LittleEndian::read_u16(&self.buffer[0..2]) as usize;

        // Sanity Check: If length is 0 or massive, it's likely garbage/crypto noise
        if packet_len < 2 || packet_len > 8192 {
            // Needed: Reset logic if we get desynced (crypto fail).
            // For now, let's just clear and wait for a fresh start.
            // println!("[Buffer] Invalid Length ({}), clearing...", packet_len);
            self.buffer.clear();
            return None;
        }

        // 3. Do we have enough data for the full packet?
        if self.buffer.len() >= packet_len {
            // Extract the packet
            let packet: Vec<u8> = self.buffer.drain(0..packet_len).collect();
            return Some(packet);
        }

        // Not enough data yet. Wait for next TCP chunk.
        None
    }
}