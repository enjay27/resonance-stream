//! Mock game server -> capture pipeline: what TCP and the game do to the
//! port-5003 stream, and the chat rows that must come out of it.

mod support;

use support::game_server::*;
use support::harness::Capture;

fn one_per_segment(frames: &[Vec<u8>]) -> Vec<Vec<u8>> {
    Connection::to_default_client().packets(frames)
}

#[test]
fn a_chat_line_arrives_with_its_sender_and_ids() {
    let line = chat(7, 37_276_266, "あずるる", "こんにちは").on(Channel::Guild);
    let rows = Capture::new().feed(&one_per_segment(&[line.frame()]));
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.message, "こんにちは");
    assert_eq!(row.channel, "GUILD");
    assert_eq!(row.nickname, "あずるる");
    assert_eq!(row.uid, 37_276_266);
    assert_eq!(row.level, 60);
    assert_eq!(row.sequence_id, 7);
    assert_eq!(row.timestamp, line.timestamp);
    assert_eq!(row.pid, 1);
    assert_eq!(row.nickname_romaji.as_deref(), Some("Azururu"));
}

#[test]
fn every_channel_is_told_apart() {
    let frames: Vec<_> = [
        Channel::World,
        Channel::Local,
        Channel::Party,
        Channel::Guild,
    ]
    .iter()
    .enumerate()
    .map(|(i, &ch)| chat(i as u64 + 1, 100, "Bob", "hi").on(ch).frame())
    .collect();
    let channels: Vec<_> = Capture::new()
        .feed(&one_per_segment(&frames))
        .into_iter()
        .map(|c| c.channel)
        .collect();
    assert_eq!(channels, ["WORLD", "LOCAL", "PARTY", "GUILD"]);
}

#[test]
fn own_lines_are_shown_as_me() {
    let frames = [
        me_frame(Channel::Party, "了解"),
        me_frame(Channel::Guild, "おつ"),
        me_frame(Channel::World, "hi"),
    ];
    let rows = Capture::new().feed(&one_per_segment(&frames));
    let got: Vec<_> = rows
        .iter()
        .map(|c| (c.nickname.as_str(), c.channel.as_str(), c.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("Me", "PARTY", "了解"),
            ("Me", "GUILD", "おつ"),
            ("Me", "WORLD", "hi")
        ]
    );
}

#[test]
fn the_same_own_line_twice_is_shown_twice() {
    // Own lines carry no ids, so they are never taken for duplicates.
    let frames = [
        me_frame(Channel::World, "gg"),
        me_frame(Channel::World, "gg"),
    ];
    assert_eq!(
        Capture::new().texts(&one_per_segment(&frames)),
        ["gg", "gg"]
    );
}

#[test]
fn a_burst_coalesced_into_one_segment_is_all_shown() {
    let burst: Vec<u8> = (1..=5)
        .flat_map(|i| chat(i, 100 + i, "Bob", &format!("line {i}")).frame())
        .collect();
    let got = Capture::new().texts(&one_per_segment(&[burst]));
    assert_eq!(got, ["line 1", "line 2", "line 3", "line 4", "line 5"]);
}

#[test]
fn a_frame_split_in_two_is_joined() {
    let frame = chat(1, 100, "Bob", "split me").frame();
    for cut in 1..frame.len() {
        let got = Capture::new().texts(&one_per_segment(&cut_at(&frame, &[cut])));
        assert_eq!(got, ["split me"], "cut at {cut} of {}", frame.len());
    }
}

#[test]
fn a_frame_sent_one_byte_at_a_time_is_joined() {
    let frame = chat(1, 100, "Bob", "slow link").frame();
    let got = Capture::new().texts(&one_per_segment(&cut_every(&frame, 1)));
    assert_eq!(got, ["slow link"]);
}

#[test]
fn japanese_cut_inside_a_character_is_joined_whole() {
    let text = "今日はいい天気ですね";
    let frame = chat(1, 100, "Bob", text).frame();
    let inside_a_char = frame.len() - text.len() + 1; // second byte of 今
    let got = Capture::new().texts(&one_per_segment(&cut_at(&frame, &[inside_a_char])));
    assert_eq!(got, [text]);
}

#[test]
fn a_long_line_with_multi_byte_lengths_is_joined() {
    // > 127 bytes: every length on the way is a two-byte varint, and the
    // frame spans several segments of a typical size.
    let text = "募集中！".repeat(60);
    let frame = chat(1, 100, "Bob", &text).frame();
    assert!(frame.len() > 700);
    let got = Capture::new().texts(&one_per_segment(&cut_every(&frame, 256)));
    assert_eq!(got, [text]);
}

#[test]
fn split_and_coalesced_frames_mixed() {
    // [a][b-first-half] then [b-second-half][c]
    let (a, b, c) = (
        chat(1, 100, "Bob", "alpha").frame(),
        chat(2, 101, "Ann", "bravo").frame(),
        chat(3, 102, "Cid", "charlie").frame(),
    );
    let stream = [a.clone(), b.clone(), c].concat();
    let cut = a.len() + b.len() / 2;
    let got = Capture::new().texts(&one_per_segment(&cut_at(&stream, &[cut])));
    assert_eq!(got, ["alpha", "bravo", "charlie"]);
}

#[test]
fn a_retransmitted_segment_is_shown_once() {
    let mut conn = Connection::to_default_client();
    let frame = chat(1, 100, "Bob", "once").frame();
    let first = conn.packet(&frame);
    let again = first.clone(); // same seq, same bytes
    let got = Capture::new().texts(&[first, again]);
    assert_eq!(got, ["once"]);
}

#[test]
fn a_line_the_server_sends_again_is_shown_once() {
    // e.g. the chat backlog re-sent on a channel switch: same uid, time, seq.
    let line = chat(9, 100, "Bob", "backlog");
    let got = Capture::new().texts(&one_per_segment(&[line.frame(), line.frame()]));
    assert_eq!(got, ["backlog"]);
}

#[test]
fn lines_reloaded_from_disk_are_not_shown_again() {
    let line = chat(9, 100, "Bob", "yesterday");
    let mut capture = Capture::new();
    let shown = capture.feed(&one_per_segment(&[line.frame()]));

    let mut after_restart = Capture::new();
    after_restart.pipeline.remember(&shown);
    assert!(after_restart
        .feed(&one_per_segment(&[line.frame()]))
        .is_empty());
}

#[test]
fn two_clients_interleaved_are_joined_per_connection() {
    let mut a = Connection::new([10, 0, 0, 1], 40_000);
    let mut b = Connection::new([10, 0, 0, 2], 40_001);
    let fa = chat(1, 100, "Bob", "from a").frame();
    let fb = chat(1, 200, "Ann", "from b").frame();
    let (a1, a2) = fa.split_at(fa.len() / 2);
    let (b1, b2) = fb.split_at(fb.len() / 2);
    let packets = [a.packet(a1), b.packet(b1), b.packet(b2), a.packet(a2)];
    assert_eq!(Capture::new().texts(&packets), ["from b", "from a"]);
}

#[test]
fn a_blocked_users_line_is_marked_and_a_resend_updates_it() {
    let line = chat(1, 666, "Spam", "buy gold");
    let mut capture = Capture::new();
    let first = capture.feed(&one_per_segment(&[line.frame()]));
    assert_eq!(first.len(), 1);
    assert!(!first[0].is_blocked);

    capture.blocked.insert(666);
    // A new line from the blocked user: shown, marked blocked.
    let next = capture.feed(&one_per_segment(&[chat(2, 666, "Spam", "cheap").frame()]));
    assert_eq!(next.len(), 1);
    assert!(next[0].is_blocked);
    // The first line re-sent: no new row, an update of the old one.
    assert!(capture.feed(&one_per_segment(&[line.frame()])).is_empty());
    assert_eq!(capture.blocked_updates.len(), 1);
    assert_eq!(capture.blocked_updates[0].pid, first[0].pid);
    assert!(capture.blocked_updates[0].is_blocked);
}

#[test]
fn stickers_and_emotes_are_shown_as_tokens() {
    let frames = [
        chat(1, 100, "Bob", "emojiPic=12").frame(),
        chat(2, 100, "Bob", "やった<sprite=3>").frame(),
    ];
    assert_eq!(
        Capture::new().texts(&one_per_segment(&frames)),
        ["[스티커]", "やった[이모지]"]
    );
}

#[test]
fn rich_lines_join_their_text_and_name_their_links() {
    let line =
        chat(1, 100, "Bob", "").rich(vec![Chunk::Text("これ売ります ".into()), Chunk::ItemLink]);
    assert_eq!(
        Capture::new().texts(&one_per_segment(&[line.frame()])),
        ["これ売ります [아이템 링크]"]
    );
}

#[test]
fn unknown_fields_are_dropped_unless_asked_for() {
    let frame = chat(1, 100, "Bob", "x").with_unknown_field().frame();
    let rows = Capture::new().feed(&one_per_segment(std::slice::from_ref(&frame)));
    assert!(rows[0].unknown_fields.is_empty());

    let mut keeping = Capture::new();
    keeping.pipeline.set_keep_unknown_fields(true);
    let rows = keeping.feed(&one_per_segment(&[frame]));
    assert!(rows[0].unknown_fields.contains_key("chat_72"));
}

#[test]
fn traffic_that_is_not_game_chat_is_ignored() {
    let frame = chat(1, 100, "Bob", "not me").frame();
    let mut other_port = Connection::to_default_client();
    other_port.server_port = 443;
    let packets = [
        other_port.packet(&frame),
        ipv6_packet(&frame),
        udp_packet(&frame),
        Connection::to_default_client().packet(&[]), // a bare ACK
        vec![0x45, 0x00, 0x00],                      // truncated IP header
        vec![],
    ];
    assert!(Capture::new().feed(&packets).is_empty());
}

#[test]
fn frames_that_are_not_chat_are_ignored_and_do_not_block_the_next() {
    // A root with neither field 2 nor 4 (other game traffic on 5003).
    let not_chat = frame(&root(var_field(1, 1)));
    let line = chat(1, 100, "Bob", "after").frame();
    let got = Capture::new().texts(&one_per_segment(&[not_chat, line]));
    assert_eq!(got, ["after"]);
}

#[test]
fn garbage_in_front_of_a_clean_segment_does_not_hide_the_next_line() {
    let line = chat(1, 100, "Bob", "clean").frame();
    let got = Capture::new().texts(&one_per_segment(&[vec![0x0A, 0xFF, 0x13, 0x37], line]));
    assert_eq!(got, ["clean"]);
}

#[test]
fn a_lost_segment_costs_its_frame_only() {
    // The middle of `lost` never arrives; the frames after it still decode.
    let lost = chat(1, 100, "Bob", "lost in transit").frame();
    let next = chat(2, 101, "Ann", "still here").frame();
    let mut conn = Connection::to_default_client();
    let (head, _missing) = lost.split_at(lost.len() / 2);
    let packets = [conn.packet(head), conn.packet(&next)];
    let got = Capture::new().texts(&packets);
    assert_eq!(got, ["still here"]);
}

#[test]
fn reordered_segments_lose_that_frame_only() {
    // Known gap (W2 in .memory/roadmap/core-review-2026-09-30.md): the
    // framing does not read TCP sequence numbers, so a frame whose halves
    // arrive swapped is dropped. Pinned so a fix shows up here; what must
    // hold today is that nothing panics and the next frame decodes.
    let swapped = chat(1, 100, "Bob", "swapped halves").frame();
    let next = chat(2, 101, "Ann", "next").frame();
    let conn = Connection::to_default_client();
    let (a, b) = swapped.split_at(swapped.len() / 2);
    let packets = [
        conn.packet_at(1_000 + a.len() as u32, b),
        conn.packet_at(1_000, a),
        conn.packet_at(1_000 + swapped.len() as u32, &next),
    ];
    assert_eq!(Capture::new().texts(&packets), ["next"]);
}

/// xorshift64*, seeded: the same cuts on every run and every OS.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n
    }
}

/// A realistic burst: Japanese and ASCII, short and long, other players'
/// and own lines, one rich line. Returns the frames and the lines they show.
fn burst() -> (Vec<Vec<u8>>, Vec<String>) {
    let texts = [
        "こんにちは",
        "116　偵察右　銀なぽ",
        "募集中！レイド@2",
        "gg",
        "今日はいい天気ですね。明日も一緒に行きましょう！",
        "了解",
    ];
    let mut frames: Vec<Vec<u8>> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| chat(i as u64 + 1, 1_000 + i as u64, "たろう", t).frame())
        .collect();
    frames.push(me_frame(Channel::Party, "おつかれ"));
    frames.push(
        chat(99, 2_000, "Bob", "")
            .rich(vec![Chunk::Text("売ります ".into()), Chunk::ItemLink])
            .frame(),
    );
    let mut lines: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
    lines.push("おつかれ".into());
    lines.push("売ります [아이템 링크]".into());
    (frames, lines)
}

#[test]
fn a_burst_coalesced_any_way_gives_the_same_lines() {
    // Segments that end on frame boundaries, holding any number of frames.
    let (frames, want) = burst();
    let mut rng = Rng(0x5EED_CAFE_F00D_0001);
    for round in 0..500 {
        let mut segments: Vec<Vec<u8>> = Vec::new();
        for frame in &frames {
            match segments.last_mut() {
                Some(last) if rng.below(2) == 0 => last.extend_from_slice(frame),
                _ => segments.push(frame.clone()),
            }
        }
        let got = Capture::new().texts(&one_per_segment(&segments));
        assert_eq!(
            got,
            want,
            "round {round}, segment sizes {:?}",
            sizes(&segments)
        );
    }
}

#[test]
fn each_plain_line_split_anywhere_gives_the_same_lines() {
    // Every frame on its own, each cut into up to 5 pieces at random. Rich
    // lines are left out: see `a_rich_line_cut_right_after_its_text_is_lost`.
    let (mut frames, mut want) = burst();
    frames.pop();
    want.pop();
    let mut rng = Rng(0x5EED_CAFE_F00D_0002);
    for round in 0..500 {
        let mut segments = Vec::new();
        for frame in &frames {
            let mut cuts: Vec<usize> = (0..rng.below(5))
                .map(|_| 1 + rng.below(frame.len() as u64 - 1) as usize)
                .collect();
            cuts.sort_unstable();
            cuts.dedup();
            segments.extend(cut_at(frame, &cuts));
        }
        let got = Capture::new().texts(&one_per_segment(&segments));
        assert_eq!(
            got,
            want,
            "round {round}, segment sizes {:?}",
            sizes(&segments)
        );
    }
}

fn sizes(segments: &[Vec<u8>]) -> Vec<usize> {
    segments.iter().map(Vec::len).collect()
}

#[test]
fn a_rich_line_cut_right_after_its_text_is_lost() {
    // Known bug (W2, framing without the real header): the text chunk is
    // `0x0A len text`, the same shape as a root. A segment that ends right
    // after the text resolves as that false root on its own, so the real
    // frame's start is not held, and the rest arrives alone. Pinned so a
    // framing fix shows up here: the fix must show the line.
    let line = chat(1, 100, "Bob", "")
        .rich(vec![Chunk::Text("売ります ".into()), Chunk::ItemLink])
        .frame();
    let text = "売ります ".as_bytes();
    let text_end = line.windows(text.len()).position(|w| w == text).unwrap() + text.len();
    let segments = cut_at(&line, &[text_end]);

    let mut warm = Capture::new();
    let mut conn = Connection::to_default_client();
    let first = chat(1, 1, "Ann", "header learned").frame();
    assert_eq!(warm.texts(&[conn.packet(&first)]), ["header learned"]);
    assert_eq!(warm.texts(&conn.packets(&segments)), Vec::<String>::new());
}

#[test]
fn a_segment_that_ends_inside_a_rich_line_can_drop_the_whole_held_burst() {
    // Known bug, found by random cuts (W2, framing without the real header):
    // no segment below ends on a frame boundary, so the first 420 bytes
    // (seven complete lines and the start of the rich one) are held. The
    // fourth segment, read on its own, looks like a 15-byte root: the text
    // chunk's 0x0A tag, whose length ends exactly where the segment does.
    // That wins over the held bytes, which are thrown away -- all 8 lines
    // are lost. Pinned so a framing fix shows up here: the fix must make
    // this show every line of `burst()`.
    let (frames, _) = burst();
    let stream = frames.concat();
    let segments = cut_at(&stream, &[246, 310, 420, 464]);
    assert_eq!(sizes(&segments), [246, 64, 110, 44, 6]);
    assert_eq!(
        Capture::new().texts(&one_per_segment(&segments)),
        Vec::<String>::new()
    );
}
