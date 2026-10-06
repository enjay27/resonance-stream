//! Property tests for what reads bytes and text from outside the app: the network
//! stream, capture and replay files, the update feed, bridge commands, and chat
//! text on its way to the translator. The input is random (and, for the stream,
//! random *near* valid), so the failures are the ones nobody wrote an example for:
//! a panic, an unbounded buffer, a result that depends on how TCP cut the bytes.
//! A failing case is shrunk and saved under `tests/properties.proptest-regressions`;
//! keep that file, it replays the case on every run.

use etherparse::PacketBuilder;
use proptest::collection::vec;
use proptest::prelude::*;
use resonance_core::capture::{decode_line, encode_line, ChatPipeline};
use resonance_core::protocol::framing::{FrameAssembler, FrameKind, MAX_FRAME_LEN};
use resonance_core::protocol::parser::{history_pipeline, parsing_pipeline};
use resonance_core::text::{
    completion_request, convert_to_romaji, normalize_emotes, postprocess_text, preprocess_text,
    Dictionary,
};

/// A well-formed frame: `[len][type][inner header][0x0A + body]`.
fn frame(ty: u16, inner: usize, body: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 4];
    out.extend(ty.to_be_bytes());
    out.extend(vec![0xEE; inner]);
    out.push(0x0A);
    out.extend_from_slice(body);
    let len = out.len() as u32;
    out[..4].copy_from_slice(&len.to_be_bytes());
    out
}

fn live(body: &[u8]) -> Vec<u8> {
    frame(0x0002, 16, body)
}

fn keepalive() -> Vec<u8> {
    vec![0, 0, 0, 6, 0, 4]
}

/// Valid frames, one after another: live chat and keepalives.
fn valid_stream() -> impl Strategy<Value = (Vec<Vec<u8>>, Vec<u8>)> {
    vec(
        prop_oneof![
            vec(any::<u8>(), 0..300).prop_map(|body| live(&body)),
            Just(keepalive()),
        ],
        1..8,
    )
    .prop_map(|frames| {
        let stream = frames.concat();
        (frames, stream)
    })
}

/// Cuts `data` after each of `cuts` (any order, repeats and out-of-range are fine).
fn segments(data: &[u8], cuts: &[usize]) -> Vec<Vec<u8>> {
    let mut points: Vec<usize> = cuts.iter().map(|c| c % (data.len() + 1)).collect();
    points.push(0);
    points.push(data.len());
    points.sort_unstable();
    points.dedup();
    points
        .windows(2)
        .map(|w| data[w[0]..w[1]].to_vec())
        .collect()
}

fn push_all(assembler: &mut FrameAssembler, segments: &[Vec<u8>]) -> Vec<(bool, Vec<u8>)> {
    let mut out = Vec::new();
    for segment in segments {
        for frame in assembler.push(segment) {
            out.push((format!("{:?}", frame.kind) == "Live", frame.root));
        }
        assert!(
            assembler.pending_len() <= MAX_FRAME_LEN,
            "the buffer is bounded"
        );
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // --- The stream ---

    #[test]
    fn the_assembler_survives_any_bytes(segs in vec(vec(any::<u8>(), 0..400), 0..12)) {
        let mut assembler = FrameAssembler::new();
        push_all(&mut assembler, &segs);
    }

    /// Valid frames with bytes flipped, dropped and inserted: the shape of a capture that lost a segment.
    #[test]
    fn the_assembler_survives_a_damaged_stream(
        (_, stream) in valid_stream(),
        damage in vec((any::<usize>(), any::<u8>(), 0u8..3), 0..6),
        cuts in vec(any::<usize>(), 0..8),
    ) {
        let mut bytes = stream;
        for (at, byte, how) in damage {
            if bytes.is_empty() { break; }
            let at = at % bytes.len();
            match how {
                0 => bytes[at] = byte,
                1 => { bytes.remove(at); }
                _ => bytes.insert(at, byte),
            }
        }
        let mut assembler = FrameAssembler::new();
        push_all(&mut assembler, &segments(&bytes, &cuts));
    }

    /// TCP may cut the stream anywhere: the frames must not depend on where.
    #[test]
    fn frames_do_not_depend_on_how_tcp_cut_the_stream(
        (frames, stream) in valid_stream(),
        cuts in vec(any::<usize>(), 0..10),
    ) {
        let whole = push_all(&mut FrameAssembler::new(), std::slice::from_ref(&stream));
        let cut = push_all(&mut FrameAssembler::new(), &segments(&stream, &cuts));
        prop_assert_eq!(&whole, &cut);
        let chats = frames.iter().filter(|f| f.len() > 6).count();
        prop_assert_eq!(whole.len(), chats, "every chat frame comes out once");
    }

    #[test]
    fn the_parsers_survive_any_bytes(data in vec(any::<u8>(), 0..600)) {
        let mut rooted = vec![0x0A];
        rooted.extend_from_slice(&data);
        for bytes in [&data, &rooted] {
            let _ = parsing_pipeline(bytes);
            let _ = history_pipeline(bytes);
        }
    }

    /// Any bytes in a packet from port 5003, and any bytes as a packet.
    #[test]
    fn the_pipeline_survives_any_packet(
        payload in vec(any::<u8>(), 0..500),
        raw in vec(any::<u8>(), 0..200),
        seq in any::<u32>(),
    ) {
        let mut pipeline = ChatPipeline::new();
        let mut pid = 0;
        let mut assign = || { pid += 1; pid };
        let mut packet = Vec::new();
        PacketBuilder::ipv4([10, 0, 0, 1], [10, 0, 0, 2], 64)
            .tcp(5003, 40000, seq, 0)
            .write(&mut packet, &payload)
            .unwrap();
        pipeline.feed_network_packet(&packet, |_| false, &mut assign, || {});
        pipeline.feed_network_packet(&raw, |_| false, &mut assign, || {});
    }

    // --- Files and feeds ---

    #[test]
    fn a_capture_line_round_trips(ts in any::<u64>(), packet in vec(any::<u8>(), 1..200)) {
        let line = encode_line(ts, &packet);
        prop_assert_eq!(decode_line(&line), Some((ts, packet)));
    }

    #[test]
    fn a_capture_line_reader_survives_any_text(text in "\\PC{0,200}") {
        let _ = decode_line(&text);
        let _ = resonance_core::capture::replay(&text);
    }

    #[test]
    fn the_feed_and_replay_and_command_readers_survive_any_text(text in "\\PC{0,300}") {
        let _ = resonance_core::update_feed::parse_feed(&text);
        let _ = resonance_core::update_feed::parse_feed_allowing(&text, true);
        let _ = resonance_core::replay::parse_replay(&text);
        let _ = resonance_core::bridge::parse_command("rs/test/command/ping", text.as_bytes());
        let _ = Dictionary::from_json_str(&text);
    }

    // --- Chat text on its way to the translator ---

    #[test]
    fn chat_text_survives_any_unicode(text in "\\PC{0,200}", reply in "\\PC{0,200}") {
        let dict = Dictionary::default();
        let shield = preprocess_text(&text, &dict, None);
        let _ = postprocess_text(&reply, &shield);
        let _ = normalize_emotes(&text);
        let _ = convert_to_romaji(&text);
        let _ = completion_request(&text);
    }

    /// What was shielded comes back: a model that returns the masked text untouched
    /// leaves no placeholder behind.
    #[test]
    fn shielded_words_come_back_after_the_round_trip(
        text in "[あ-んア-ン一-龥「」『』（）a-zA-Z0-9 ！？、。]{0,60}",
    ) {
        let shield = preprocess_text(&text, &Dictionary::default(), None);
        let restored = postprocess_text(&shield.masked_text, &shield);
        prop_assert!(!restored.contains("[P"), "placeholder left: {restored:?} from {text:?}");
    }
}
