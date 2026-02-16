// src-tauri/src/packet_buffer.rs

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct PacketBuffer {
    buffer: Vec<u8>,
    last_success_ms: u64, // [NEW] Timestamp of last successful parse
}

impl PacketBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(65536), // Increased buffer size (64KB)
            last_success_ms: get_timestamp(),
        }
    }

    pub fn add(&mut self, data: &[u8]) {
        let now = get_timestamp();

        // [NEW] WATCHDOG LOGIC
        // If the buffer is NOT empty (meaning we are accumulating data)
        // AND it has been > 500 ms since we successfully parsed a packet...
        if !self.buffer.is_empty() {
            if now.saturating_sub(self.last_success_ms) > 500 {
                println!("[PacketBuffer] Watchdog: Stuck buffer detected (len={}). Resetting.", self.buffer.len());
                self.buffer.clear();
                self.last_success_ms = now; // Reset timer
            }
        } else {
            // If buffer was empty, this is the start of a new stream/packet.
            // Reset timer so we don't clear valid data immediately after a long idle period.
            self.last_success_ms = now;
        }

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
            // [CHANGED] Increased safety limit from 8192 to 65536
            if self.buffer.len() > 65536 {
                self.buffer.clear();
            }
            return None;
        }

        // 2. We need at least 2 bytes to read the Tag + Varint Length
        if self.buffer.len() < 2 { return None; }

        // 3. Read the Varint length right after the 0x0A
        let (msg_len, varint_size) = read_varint_safe(&self.buffer[1..]);

        // If varint_size is 0, it means the Varint was split across TCP packets
        if varint_size == 0 { return None; }

        // Total packet length = 1 byte (0x0A) + Varint size + the actual message length
        let total_packet_len = 1 + varint_size + msg_len as usize;

        // Sanity Check
        if total_packet_len > 65535 {
            self.buffer.drain(0..1); // Pop invalid 0x0A and retry
            return None;
        }

        // 4. Do we have the full assembled packet?
        if self.buffer.len() >= total_packet_len {
            // Extract the full Protobuf stream
            let packet: Vec<u8> = self.buffer.drain(0..total_packet_len).collect();

            // [NEW] SUCCESS! Update the watchdog timestamp.
            self.last_success_ms = get_timestamp();

            return Some(packet);
        }

        // Not enough data yet. TCP segmentation happened.
        None
    }
}

fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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