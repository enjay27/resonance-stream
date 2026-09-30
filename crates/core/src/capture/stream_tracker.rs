use crate::protocol::framing::FrameAssembler;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A TCP connection: server address + port, then client address + port. The
/// client half matters: two game clients on one PC talk to the same server
/// address and port over two connections.
pub type StreamKey = [u8; 12];

/// A connection silent this long is forgotten, so the map does not grow by
/// one entry per connection seen. (Half-received frames expire far sooner,
/// inside the assembler.)
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// How often the idle check runs; it walks the whole map.
const PRUNE_EVERY: Duration = Duration::from_secs(10);

struct Stream {
    assembler: FrameAssembler,
    last_seen: Instant,
}

/// One frame assembler per TCP connection.
pub struct StreamTracker {
    streams: HashMap<StreamKey, Stream>,
    last_prune: Option<Instant>,
}

impl StreamTracker {
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
            last_prune: None,
        }
    }

    /// Takes raw bytes from a specific connection and returns fully assembled packets
    pub fn process_bytes(&mut self, stream_key: StreamKey, payload: &[u8]) -> Vec<Vec<u8>> {
        self.process_bytes_at(stream_key, payload, Instant::now())
    }

    /// [`process_bytes`](Self::process_bytes) with an explicit clock, for tests.
    pub fn process_bytes_at(
        &mut self,
        stream_key: StreamKey,
        payload: &[u8],
        now: Instant,
    ) -> Vec<Vec<u8>> {
        let last_prune = *self.last_prune.get_or_insert(now);
        if now.saturating_duration_since(last_prune) >= PRUNE_EVERY {
            self.streams.retain(|_, stream| {
                now.saturating_duration_since(stream.last_seen) <= STREAM_IDLE_TIMEOUT
            });
            self.last_prune = Some(now);
        }
        let stream = self.streams.entry(stream_key).or_insert_with(|| Stream {
            assembler: FrameAssembler::new(),
            last_seen: now,
        });
        stream.last_seen = now;
        stream.assembler.push_at(payload, now)
    }

    /// Connections currently tracked.
    #[cfg(test)]
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_streams_are_dropped() {
        // W1: the map only ever grew (one entry per connection seen).
        let mut tracker = StreamTracker::new();
        let t0 = Instant::now();
        tracker.process_bytes_at(key(1), &[0], t0);
        tracker.process_bytes_at(key(2), &[0], t0);
        assert_eq!(tracker.stream_count(), 2);

        // Much later only stream 2 is active: stream 1 is forgotten.
        let later = t0 + STREAM_IDLE_TIMEOUT + Duration::from_secs(1);
        tracker.process_bytes_at(key(2), &[0], later);
        assert_eq!(tracker.stream_count(), 1);
    }

    fn key(n: u8) -> StreamKey {
        [n; 12]
    }
}
