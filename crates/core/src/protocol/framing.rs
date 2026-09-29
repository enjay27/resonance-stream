//! Finds protobuf chat roots in a port-5003 TCP stream.
//!
//! The application frame header in front of each root is not decoded (its
//! layout is not known), so a root is recognised by its shape: a `0x0A` tag
//! whose varint length makes the root end *exactly* at the end of the data
//! seen so far. That exact-end rule is what keeps random `0x0A` bytes inside
//! other data from being taken for a root.
//!
//! On top of that rule, the assembler handles what TCP does to the stream:
//! - several frames in one segment: `[hdr][root][hdr][root]` resolves as a
//!   chain of roots separated by headers of one fixed size;
//! - one frame over several segments: bytes that do not resolve yet are kept
//!   (bounded in size and age) and retried with the next segment.

use crate::protocol::decoder::read_varint;
use std::ops::Range;
use std::time::{Duration, Instant};

const ROOT_TAG: u8 = 0x0A;
/// Unresolved bytes kept per stream while waiting for the rest of a frame.
pub const MAX_PENDING: usize = 64 * 1024;
/// A frame's segments arrive within milliseconds of each other; anything
/// older than this is a lost cause (a missed segment), not a slow one.
pub const PENDING_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
pub struct FrameAssembler {
    pending: Vec<u8>,
    pending_since: Option<Instant>,
    /// Header size of the last frame that resolved on this stream.
    header: Option<usize>,
}

impl FrameAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one TCP payload; returns every complete root it completes.
    pub fn push(&mut self, segment: &[u8]) -> Vec<Vec<u8>> {
        self.push_at(segment, Instant::now())
    }

    /// [`push`](Self::push) with an explicit clock, for tests.
    pub fn push_at(&mut self, segment: &[u8], now: Instant) -> Vec<Vec<u8>> {
        if segment.is_empty() {
            return Vec::new();
        }
        if self
            .pending_since
            .is_some_and(|since| now.saturating_duration_since(since) > PENDING_TIMEOUT)
        {
            self.clear();
        }

        // Two readings of the new bytes: they continue an unfinished frame
        // (pending + segment), or they stand on their own.
        let joined = (!self.pending.is_empty()).then(|| [&self.pending[..], segment].concat());
        let as_continuation = joined
            .as_deref()
            .and_then(|data| best_chain(data, self.header));
        let on_its_own = best_chain(segment, self.header);

        // The more plausible framing wins: the header size already seen on
        // this stream, else the smaller header. A tie goes to the continuation.
        let rank = |header: usize| (Some(header) != self.header, header);
        let chosen = match (as_continuation, on_its_own) {
            (Some(c), Some(o)) if rank(o.0) < rank(c.0) => Some((segment, o)),
            (Some(c), _) => Some((joined.as_deref().unwrap_or_default(), c)),
            (None, Some(o)) => Some((segment, o)),
            (None, None) => None,
        };
        if let Some((data, (header, roots))) = chosen {
            let out = roots.into_iter().map(|r| data[r].to_vec()).collect();
            self.clear();
            self.header = Some(header);
            return out;
        }

        // Not resolvable yet: keep the bytes for the next segment.
        match joined {
            Some(joined) => self.pending = joined,
            None => {
                self.pending.extend_from_slice(segment);
                self.pending_since = Some(now);
            }
        }
        if self.pending.len() > MAX_PENDING {
            // Too much unresolved data in front: restart from this segment alone.
            let tail = &segment[segment.len().saturating_sub(MAX_PENDING)..];
            self.pending = tail.to_vec();
            self.pending_since = Some(now);
        }
        Vec::new()
    }

    /// Bytes waiting for the rest of a frame.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.pending_since = None;
    }
}

/// Splits `data` into `[hdr][root][hdr][root]...` where every header has the
/// same size and the last root ends exactly at `data.len()`. Tries the
/// `preferred` header size first, then sizes smallest first. Returns the
/// header size used and the root ranges, or `None` if no size explains the
/// whole buffer.
fn best_chain(data: &[u8], preferred: Option<usize>) -> Option<(usize, Vec<Range<usize>>)> {
    preferred
        .into_iter()
        .chain((0..data.len()).filter(|&header| data[header] == ROOT_TAG))
        .find_map(|header| root_chain(data, header).map(|roots| (header, roots)))
}

fn root_chain(data: &[u8], header: usize) -> Option<Vec<Range<usize>>> {
    let mut roots = Vec::new();
    let mut start = header;
    loop {
        if *data.get(start)? != ROOT_TAG {
            return None;
        }
        let (len, varint_size) = read_varint(&data[start + 1..]);
        if varint_size == 0 {
            return None;
        }
        let end = usize::try_from(len)
            .ok()
            .and_then(|len| (start + 1 + varint_size).checked_add(len))?;
        if end > data.len() {
            return None;
        }
        roots.push(start..end);
        if end == data.len() {
            return Some(roots);
        }
        start = end.checked_add(header)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HDR: [u8; 4] = [0x00, 0x00, 0x11, 0x22];

    fn root(body: &[u8]) -> Vec<u8> {
        let mut r = vec![ROOT_TAG, body.len() as u8];
        r.extend_from_slice(body);
        r
    }

    fn frame(body: &[u8]) -> Vec<u8> {
        [&HDR[..], &root(body)].concat()
    }

    #[test]
    fn one_frame_per_segment() {
        let mut fa = FrameAssembler::new();
        assert_eq!(fa.push(&frame(&[0xBB, 0xCC])), vec![root(&[0xBB, 0xCC])]);
        assert_eq!(fa.pending_len(), 0);
    }

    #[test]
    fn root_claiming_more_than_is_there_is_held_not_emitted() {
        // Ported from strip_application_header's rejection case.
        let mut fa = FrameAssembler::new();
        assert!(fa.push(&[0x00, 0x11, 0x22, 0x0A, 0x09, 0xBB]).is_empty());
        assert_eq!(fa.pending_len(), 6);
    }

    #[test]
    fn coalesced_frames_resolve_as_a_chain() {
        let mut fa = FrameAssembler::new();
        let seg = [frame(b"one"), frame(b"two"), frame(b"three")].concat();
        assert_eq!(
            fa.push(&seg),
            vec![root(b"one"), root(b"two"), root(b"three")]
        );
    }

    #[test]
    fn frame_split_over_segments_is_joined() {
        let mut fa = FrameAssembler::new();
        let whole = frame(b"hello world");
        let (a, b) = whole.split_at(7);
        assert!(fa.push(a).is_empty());
        assert_eq!(fa.push(b), vec![root(b"hello world")]);
        assert_eq!(fa.pending_len(), 0);
    }

    #[test]
    fn retransmitted_first_half_does_not_block_the_frame() {
        let mut fa = FrameAssembler::new();
        let whole = frame(b"hello world");
        let (a, b) = whole.split_at(7);
        assert!(fa.push(a).is_empty());
        assert!(fa.push(a).is_empty());
        assert_eq!(fa.push(b), vec![root(b"hello world")]);
    }

    #[test]
    fn garbage_in_front_does_not_hide_a_clean_segment() {
        let mut fa = FrameAssembler::new();
        assert!(fa.push(&[0x0A, 0x7F, 1, 2, 3]).is_empty()); // never completes
        let seg = [frame(b"one"), frame(b"two")].concat();
        assert_eq!(fa.push(&seg), vec![root(b"one"), root(b"two")]);
        assert_eq!(fa.pending_len(), 0);
    }

    #[test]
    fn stale_pending_bytes_are_dropped() {
        let mut fa = FrameAssembler::new();
        let t0 = Instant::now();
        let whole = frame(b"hello world");
        let (a, b) = whole.split_at(7);
        assert!(fa.push_at(a, t0).is_empty());
        // The second half arrives after the timeout: the halves are not joined.
        let late = t0 + PENDING_TIMEOUT + Duration::from_millis(1);
        assert!(fa.push_at(b, late).is_empty());
        assert_eq!(fa.pending_len(), b.len());
    }

    #[test]
    fn pending_is_bounded() {
        let mut fa = FrameAssembler::new();
        // A root start claiming 65535 bytes, then zeros: never ends exactly.
        let mut junk = vec![0x0A, 0xFF, 0xFF, 0x03];
        junk.resize(4096, 0);
        for _ in 0..40 {
            assert!(fa.push(&junk).is_empty());
            assert!(fa.pending_len() <= MAX_PENDING);
        }
        assert!(fa.pending_len() > 0);
    }

    #[test]
    fn learned_header_size_beats_a_spurious_smaller_one() {
        let mut fa = FrameAssembler::new();
        assert_eq!(fa.push(&frame(b"x")), vec![root(b"x")]); // learns header = 4
                                                             // This header happens to contain `0A 00` (an empty root), so the
                                                             // segment also reads as a chain with 1-byte headers:
                                                             // [00] [0A 00] [00] [0A 02 BB CC].
        let seg = [0x00, 0x0A, 0x00, 0x00, 0x0A, 0x02, 0xBB, 0xCC];
        assert_eq!(fa.push(&seg), vec![root(&[0xBB, 0xCC])]);
    }

    #[test]
    fn huge_length_does_not_overflow() {
        let mut seg = vec![0x0A];
        seg.extend([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
        assert!(FrameAssembler::new().push(&seg).is_empty());
    }
}
