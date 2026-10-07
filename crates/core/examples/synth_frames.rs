//! Helper for the synthetic-capture test (`runbook/runbook/pipelines/capture_spike.py`):
//! what the game server would send, and what the app's firewall rule is called.
//!
//!     cargo run -q -p resonance-core --example synth_frames -- frames <replay.jsonl>
//!         one JSON object per chat line of a `--replay-chat` file:
//!         {"delay_ms", "hex" (the live frame), "channel", "nickname", "uid", "level", "text"}
//!     cargo run -q -p resonance-core --example synth_frames -- rule-name <exe path>
//!         the name of the firewall rule the app makes for that exe

use resonance_core::capture::synth::live_chat_frame;
use resonance_core::replay::parse_replay;
use resonance_core::sniffer_net::rule_name_for;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["frames", path] => frames(path),
        ["rule-name", exe] => {
            println!("{}", rule_name_for(exe));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: synth_frames frames <replay.jsonl> | rule-name <exe path>");
            ExitCode::from(2)
        }
    }
}

fn frames(path: &str) -> ExitCode {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => return fail(format!("cannot read {path}: {e}")),
    };
    let entries = match parse_replay(&text) {
        Ok(entries) => entries,
        Err(e) => return fail(format!("{path}: {e}")),
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for (index, entry) in entries.iter().enumerate() {
        let chat = entry.into_chat(index, now);
        let hex: String = live_chat_frame(&chat)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "delay_ms": entry.delay_ms,
                "hex": hex,
                "channel": chat.channel.as_str(),
                "nickname": chat.nickname,
                "uid": chat.uid,
                "level": chat.level,
                "text": chat.message,
            })
        );
    }
    ExitCode::SUCCESS
}

fn fail(message: String) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}
