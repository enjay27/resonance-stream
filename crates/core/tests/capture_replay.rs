//! Replays real captures (`tests/fixtures/*.capture.log`, made with the app's
//! debug "Raw Capture" and kept out of git) through the pipeline. With no
//! capture present this passes vacuously; W2 is finished by extending it with
//! what the messages seen in game should have been.

use resonance_core::capture::{replay, ChatPipeline, PipelineAction};
use std::fs;

#[test]
fn real_captures_replay_without_panicking() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let is_capture = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".capture.log"));
        if !is_capture {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let mut pipeline = ChatPipeline::new();
        let (mut packets, mut chats) = (0, 0);
        let mut pid = 0;
        for packet in replay(&text) {
            packets += 1;
            chats += pipeline
                .feed_network_packet(
                    &packet,
                    |_| false,
                    || {
                        pid += 1;
                        pid
                    },
                    || {},
                )
                .iter()
                .filter(|a| matches!(a, PipelineAction::EmitNewMessage(_)))
                .count();
        }
        println!("{}: {packets} packets -> {chats} chats", path.display());
        assert!(packets > 0, "{} has no usable packets", path.display());
    }
}
