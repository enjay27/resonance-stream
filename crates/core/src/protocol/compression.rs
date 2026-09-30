//! zstd for the compressed frames (type bit 15) of the port-5003 stream.

use ruzstd::decoding::StreamingDecoder;
use std::io::Read;

/// The most a frame may inflate to. Real frames are a few KB; the cap stops a
/// corrupt or hostile stream from asking for gigabytes.
pub const MAX_DECOMPRESSED: usize = 1 << 20;

/// Inflates one zstd frame; `None` if it is not valid zstd or would inflate
/// past [`MAX_DECOMPRESSED`].
pub fn decompress(data: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = StreamingDecoder::new(data).ok()?;
    let mut out = Vec::new();
    // One byte over the cap tells "exactly the cap" from "more than the cap".
    let limit = MAX_DECOMPRESSED as u64 + 1;
    decoder.by_ref().take(limit).read_to_end(&mut out).ok()?;
    (out.len() <= MAX_DECOMPRESSED).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruzstd::encoding::{compress_to_vec, CompressionLevel};

    /// Made by the reference zstd (python-zstandard, level 3), so this pins
    /// interop with the encoder the game server uses, not just our own.
    const REFERENCE: &str =
        "28b52ffd00000501009068656c6c6f202c206b6f6e6e696368697761040072d4043080a0bc94b91401";

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn a_reference_zstd_frame_inflates() {
        let want = "hello hello hello hello, konnichiwa ".repeat(4);
        assert_eq!(decompress(&unhex(REFERENCE)), Some(want.into_bytes()));
    }

    #[test]
    fn our_own_encoder_round_trips() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let packed = compress_to_vec(&data[..], CompressionLevel::Fastest);
        assert_eq!(decompress(&packed), Some(data));
    }

    #[test]
    fn what_is_not_zstd_is_refused() {
        assert_eq!(decompress(b""), None);
        assert_eq!(decompress(b"not zstd at all"), None);
    }

    #[test]
    fn a_truncated_frame_is_refused() {
        let packed = unhex(REFERENCE);
        assert_eq!(decompress(&packed[..packed.len() - 6]), None);
    }

    #[test]
    fn inflating_past_the_cap_is_refused() {
        let bomb = compress_to_vec(
            &vec![0u8; MAX_DECOMPRESSED + 1][..],
            CompressionLevel::Fastest,
        );
        assert!(bomb.len() < 4096, "a zero run should compress well");
        assert_eq!(decompress(&bomb), None);
        let at_cap = compress_to_vec(&vec![0u8; MAX_DECOMPRESSED][..], CompressionLevel::Fastest);
        assert_eq!(decompress(&at_cap).map(|d| d.len()), Some(MAX_DECOMPRESSED));
    }
}
