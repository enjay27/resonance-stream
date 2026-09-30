use crate::protocol::framing::{Frame, FrameAssembler};
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
    /// TCP sequence number of the next byte expected; `None` before the first.
    next_seq: Option<u32>,
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

    /// Takes one TCP payload (with its sequence number) from a specific
    /// connection and returns the frames it completes. Bytes seen before (a
    /// retransmit, or the overlap of a partly resent segment) are dropped; a
    /// gap (a missed or reordered segment) cuts the stream, so the frame it
    /// broke is lost and framing starts again at this segment.
    pub fn process_bytes(&mut self, stream_key: StreamKey, seq: u32, payload: &[u8]) -> Vec<Frame> {
        self.process_bytes_at(stream_key, seq, payload, Instant::now())
    }

    /// [`process_bytes`](Self::process_bytes) with an explicit clock, for tests.
    pub fn process_bytes_at(
        &mut self,
        stream_key: StreamKey,
        seq: u32,
        payload: &[u8],
        now: Instant,
    ) -> Vec<Frame> {
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
            next_seq: None,
        });
        stream.last_seen = now;

        let (mut start, mut payload) = (seq, payload);
        if let Some(next) = stream.next_seq {
            // Signed distance on the wrapping sequence space.
            let ahead = seq.wrapping_sub(next) as i32;
            if ahead < 0 {
                let seen = ahead.unsigned_abs() as usize;
                if seen >= payload.len() {
                    return Vec::new(); // all of it was delivered already
                }
                (start, payload) = (next, &payload[seen..]);
            } else if ahead > 0 {
                stream.assembler.reset();
            }
        }
        stream.next_seq = Some(start.wrapping_add(payload.len() as u32));
        stream.assembler.push(payload)
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
    use crate::protocol::framing::FrameKind;

    fn key(n: u8) -> StreamKey {
        [n; 12]
    }

    /// One live frame carrying `body` as its root's content.
    fn live(body: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 0, 0, 0, 2];
        out.extend([0u8; 16]);
        out.extend([0x0A, body.len() as u8]);
        out.extend_from_slice(body);
        let len = out.len() as u32;
        out[..4].copy_from_slice(&len.to_be_bytes());
        out
    }

    fn root(body: &[u8]) -> Vec<u8> {
        [&[0x0A, body.len() as u8][..], body].concat()
    }

    fn roots(frames: Vec<Frame>) -> Vec<Vec<u8>> {
        assert!(frames.iter().all(|f| f.kind == FrameKind::Live));
        frames.into_iter().map(|f| f.root).collect()
    }

    #[test]
    fn idle_streams_are_dropped() {
        // W1: the map only ever grew (one entry per connection seen).
        let mut tracker = StreamTracker::new();
        let t0 = Instant::now();
        tracker.process_bytes_at(key(1), 0, &[0], t0);
        tracker.process_bytes_at(key(2), 0, &[0], t0);
        assert_eq!(tracker.stream_count(), 2);

        // Much later only stream 2 is active: stream 1 is forgotten.
        let later = t0 + STREAM_IDLE_TIMEOUT + Duration::from_secs(1);
        tracker.process_bytes_at(key(2), 1, &[0], later);
        assert_eq!(tracker.stream_count(), 1);
    }

    #[test]
    fn a_retransmitted_segment_is_dropped() {
        let mut tracker = StreamTracker::new();
        let f = live(b"once");
        assert_eq!(
            roots(tracker.process_bytes(key(1), 1000, &f)),
            [root(b"once")]
        );
        assert!(tracker.process_bytes(key(1), 1000, &f).is_empty());
    }

    #[test]
    fn a_partly_resent_segment_only_adds_its_new_bytes() {
        let mut tracker = StreamTracker::new();
        let f = live(b"split");
        let (a, b) = f.split_at(10);
        assert!(tracker.process_bytes(key(1), 1000, a).is_empty());
        // The sender resends `a` together with the rest.
        let got = tracker.process_bytes(key(1), 1000, &f);
        assert_eq!(roots(got), [root(b"split")]);
        assert!(tracker
            .process_bytes(key(1), 1000 + a.len() as u32, b)
            .is_empty());
    }

    #[test]
    fn a_gap_costs_the_broken_frame_only() {
        let mut tracker = StreamTracker::new();
        let lost = live(b"lost");
        let next = live(b"next");
        assert!(tracker.process_bytes(key(1), 1000, &lost[..8]).is_empty());
        // The rest of `lost` never arrives; `next` starts further on.
        let got = tracker.process_bytes(key(1), 1000 + lost.len() as u32, &next);
        assert_eq!(roots(got), [root(b"next")]);
    }

    #[test]
    fn sequence_numbers_wrap() {
        let mut tracker = StreamTracker::new();
        let (a, b) = (live(b"one"), live(b"two"));
        let start = u32::MAX - 5;
        assert_eq!(
            roots(tracker.process_bytes(key(1), start, &a)),
            [root(b"one")]
        );
        let after = start.wrapping_add(a.len() as u32);
        assert!(after < 1000, "the second segment is past the wrap");
        assert_eq!(
            roots(tracker.process_bytes(key(1), after, &b)),
            [root(b"two")]
        );
        // ...and a retransmit of the first one, from before the wrap, is old.
        assert!(tracker.process_bytes(key(1), start, &a).is_empty());
    }

    #[test]
    fn connections_do_not_share_sequence_numbers() {
        let mut tracker = StreamTracker::new();
        let f = live(b"x");
        assert_eq!(tracker.process_bytes(key(1), 500, &f).len(), 1);
        assert_eq!(tracker.process_bytes(key(2), 500, &f).len(), 1);
    }
}
