// src-tauri/src/packet_buffer.rs

#[derive(Debug)]
pub struct PacketBuffer {
    buffer: Vec<u8>,
}

impl PacketBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(16384), // Slightly larger for MMOs
        }
    }

    pub fn add(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub fn next(&mut self) -> Option<Vec<u8>> {
        // 1. Find the start of a Chat Packet (0x0A)
        if let Some(start_idx) = self.buffer.iter().position(|&b| b == 0x0A) {
            // If there's garbage before 0x0A, drop it (Sliding Window Recovery)
            if start_idx > 0 {
                self.buffer.drain(0..start_idx);
            }
        } else {
            // No 0x0A found. Wait for more data.
            // Clear if it gets too big to prevent memory leaks from encrypted noise.
            // SAFETY VALVE: If buffer is huge but we found no valid packet, clear it.
            // This recovers from "Out of Order" corruption by resetting the stream.
            if self.buffer.len() > 8192 {
                println!("[Buffer] Resetting corrupted buffer (len > 8192)");
                self.buffer.clear();
                return None;
            }
        }

        // 2. We need at least 2 bytes to read the Tag + Varint Length
        if self.buffer.len() < 2 { return None; }

        // 3. Read the Varint length right after the 0x0A
        let (msg_len, varint_size) = read_varint_safe(&self.buffer[1..]);

        // If varint_size is 0, it means the Varint was split across TCP packets (very rare but possible).
        if varint_size == 0 { return None; }

        // Total packet length = 1 byte (0x0A) + Varint size + the actual message length
        let total_packet_len = 1 + varint_size + msg_len as usize;

        // Sanity Check
        if total_packet_len > 65535 {
            // This wasn't a real chat packet, just a random 0x0A in the network stream.
            // Pop the first byte so the `.position()` search continues on the next loop.
            self.buffer.drain(0..1);
            return None;
        }

        // 4. Do we have the full assembled packet?
        if self.buffer.len() >= total_packet_len {
            // Extract the full Protobuf stream
            let packet: Vec<u8> = self.buffer.drain(0..total_packet_len).collect();
            return Some(packet);
        }

        // Not enough data yet. TCP segmentation happened. Wait for the next chunk!
        None
    }
}

/// A safe Varint reader that returns (0,0) if the buffer ends before the Varint is finished
fn read_varint_safe(data: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    let mut pos = 0;

    while pos < data.len() {
        let byte = data[pos];
        value |= ((byte & 0x7F) as u64) << shift;
        pos += 1;

        if (byte & 0x80) == 0 {
            return (value, pos);
        }

        shift += 7;
        if shift >= 64 { break; } // Prevent panic on corrupted data
    }

    // If we exit the loop but the last byte had the continuation bit set,
    // we don't have the full Varint yet.
    (0, 0)
}