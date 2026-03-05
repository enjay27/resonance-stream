use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct PacketBuffer {
    buffer: Vec<u8>,
    last_success_ms: u64, // Timestamp of last successful parse
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

        // WATCHDOG LOGIC
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_varint_safe() {
        // Test valid 1-byte varint (Value: 5)
        let data1 = [0x05];
        let (val1, len1) = read_varint_safe(&data1);
        assert_eq!(val1, 5);
        assert_eq!(len1, 1);

        // Test valid 2-byte varint (Value: 150 -> 0x96 0x01)
        let data2 = [0x96, 0x01];
        let (val2, len2) = read_varint_safe(&data2);
        assert_eq!(val2, 150);
        assert_eq!(len2, 2);

        // Test INCOMPLETE varint (Missing the second byte)
        let data3 = [0x96];
        let (val3, len3) = read_varint_safe(&data3);
        assert_eq!(val3, 0);
        assert_eq!(len3, 0); // Should return 0 length indicating "need more data"
    }

    #[test]
    fn test_packet_buffer_reassembly() {
        let mut pb = PacketBuffer::new();

        // Construct a fake protobuf packet
        // 0x0A (Start), 0x03 (Varint Length 3), [0x01, 0x02, 0x03] (Payload)

        // 1. Add partial data
        pb.add(&[0x0A, 0x03, 0x01]);
        assert_eq!(pb.next(), None); // Packet is incomplete, should return None

        // 2. Add the rest of the data
        pb.add(&[0x02, 0x03]);
        let assembled = pb.next().unwrap();

        // 3. Verify exact extraction
        assert_eq!(assembled, vec![0x0A, 0x03, 0x01, 0x02, 0x03]);

        // Buffer should now be empty
        assert_eq!(pb.next(), None);
    }

    #[test]
    fn test_buffer_edge_cases() {
        let mut pb = PacketBuffer::new();

        // Edge Case 1: 0x0A Spam with recovery
        // 1. We start with a fake 0x0A and a length byte (0xFF 0x08) that decodes to 1151.
        // 2. We add 1100 bytes of padding.
        // 3. We add the real packet [0x0A, 0x03, 0x01, 0x02, 0x03].
        let mut data = vec![0x0A, 0xFF, 0x08]; // Total packet length will be 1154
        data.extend(vec![0; 1100]);           // Current buffer size becomes ~1108
        data.extend(&[0x0A, 0x03, 0x01, 0x02, 0x03]);

        pb.add(&data);

        // Now, next() will:
        // - Find the first 0x0A.
        // - Calculate total_len = 1154.
        // - See that total_len (1154) > buffer.len() (1108) AND buffer.len() > 1024.
        // - Trigger the sanity check, drain(0..1), and retry!
        let mut found_packet = None;
        while let Some(p) = pb.next() {
            found_packet = Some(p);
        }

        // Assert that we successfully recovered and found the real packet
        assert_eq!(found_packet.unwrap(), vec![0x0A, 0x03, 0x01, 0x02, 0x03]);

        // Edge Case 2: Insanely large fake Varint length
        pb.buffer.clear();
        // 0x0A followed by a varint that decodes to ~2 million bytes
        pb.add(&[0x0A, 0xFF, 0xFF, 0x7F, 0x00, 0x00]);

        // This triggers the `total_len > 65535` check immediately.
        assert_eq!(pb.next(), None);
    }
}