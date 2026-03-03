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
        while self.buffer.len() >= 3 {
            // 1. Find the next possible start (0x0A)
            let start_pos = self.buffer.iter().position(|&b| b == 0x0A);

            if let Some(idx) = start_pos {
                // Drop any garbage before the 0x0A
                if idx > 0 { self.buffer.drain(0..idx); }

                // 2. Read the Varint length
                let (msg_len, varint_size) = read_varint_safe(&self.buffer[1..]);
                if varint_size == 0 { return None; } // Need more data

                let total_len = 1 + varint_size + msg_len as usize;

                // 3. SANITY CHECK:
                // If the buffer length is 471 but the packet claims to be 32,000,
                // the 0x0A was a 'fake' one from the header.
                if total_len > 65535 || (total_len > self.buffer.len() && self.buffer.len() > 1024) {
                    self.buffer.drain(0..1); // Discard fake 0x0A and retry
                    continue;
                }

                // 4. Extract full packet if available
                if self.buffer.len() >= total_len {
                    let packet: Vec<u8> = self.buffer.drain(0..total_len).collect();
                    self.last_success_ms = get_timestamp();
                    return Some(packet);
                }
                return None; // Wait for TCP segmentation to complete
            } else {
                self.buffer.clear();
                return None;
            }
        }
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
pub(crate) fn read_varint_safe(data: &[u8]) -> (u64, usize) {
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