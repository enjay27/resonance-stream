//! `--replay-chat <file>`: chat lines fed into the app as if they had been
//! captured, so the chat list, the translator and the archive can be tested
//! without the game.
//!
//! The file is JSON Lines: one object per line, `text` required.
//!
//! | field | meaning | default |
//! |---|---|---|
//! | `text` | the chat message | (required) |
//! | `delay_ms` | wait this long after the previous line | 0 |
//! | `channel` | `WORLD` `GUILD` `PARTY` `LOCAL` `BEGINNER`, or the tab's Korean label | `WORLD` |
//! | `nickname` | who says it | `Tester` |
//! | `uid` | the sender's id (what blocking goes by) | a stable number made from the nickname |
//! | `class_id`, `level` | as the game sends them | 0 |
//! | `timestamp` | seconds since the epoch | when the line is replayed |
//! | `sequence_id` | with `uid` and `timestamp`, what makes two lines "the same message" | the line's place in the file |
//!
//! Blank lines and lines starting with `#` are skipped. A typo is an error with
//! its line number: a replay that quietly ignored a misspelt field would test
//! the wrong thing.

use resonance_types::{Channel, ChatMessage};
use serde_json::{Map, Value};
use std::fmt;

/// A single `delay_ms` above this is taken for a mistake (ten minutes).
pub const MAX_DELAY_MS: u64 = 600_000;

const FIELDS: [&str; 9] = [
    "text",
    "delay_ms",
    "channel",
    "nickname",
    "uid",
    "class_id",
    "level",
    "timestamp",
    "sequence_id",
];

const DEFAULT_NICKNAME: &str = "Tester";

/// One line of a replay file, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEntry {
    pub delay_ms: u64,
    pub channel: Channel,
    pub nickname: String,
    pub uid: Option<u64>,
    pub text: String,
    pub class_id: u64,
    pub level: u64,
    pub timestamp: Option<u64>,
    pub sequence_id: Option<u64>,
}

/// What was wrong, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError {
    /// 1-based line of the file.
    pub line: usize,
    pub reason: String,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for ReplayError {}

/// A stable sender id for a nickname, for lines that give none. Under 2^52, so
/// it survives a trip through JavaScript numbers; never 0.
pub fn default_uid(nickname: &str) -> u64 {
    // FNV-1a over the bytes.
    let hash = nickname.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    let id = hash & 0x000F_FFFF_FFFF_FFFF;
    if id == 0 {
        1
    } else {
        id
    }
}

/// Reads a whole replay file.
pub fn parse_replay(text: &str) -> Result<Vec<ReplayEntry>, ReplayError> {
    let mut entries = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let number = index + 1;
        let fail = |reason: String| ReplayError {
            line: number,
            reason,
        };
        let value: Value =
            serde_json::from_str(line).map_err(|e| fail(format!("not valid JSON ({e})")))?;
        let Value::Object(object) = value else {
            return Err(fail("not a JSON object".to_string()));
        };
        entries.push(entry_from(&object).map_err(fail)?);
    }
    Ok(entries)
}

fn entry_from(object: &Map<String, Value>) -> Result<ReplayEntry, String> {
    if let Some(unknown) = object.keys().find(|key| !FIELDS.contains(&key.as_str())) {
        return Err(format!(
            "unknown field {unknown:?} (known: {})",
            FIELDS.join(", ")
        ));
    }
    let text = match object.get("text") {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        Some(Value::String(_)) => return Err("\"text\" is empty".to_string()),
        Some(_) => return Err("\"text\" must be a string".to_string()),
        None => return Err("\"text\" is missing".to_string()),
    };
    let delay_ms = number(object, "delay_ms")?.unwrap_or(0);
    if delay_ms > MAX_DELAY_MS {
        return Err(format!(
            "\"delay_ms\" {delay_ms} is over {MAX_DELAY_MS} (ten minutes): a mistake?"
        ));
    }
    let channel = match object.get("channel") {
        None => Channel::World,
        Some(Value::String(name)) => channel_named(name)?,
        Some(_) => return Err("\"channel\" must be a string".to_string()),
    };
    let nickname = match object.get("nickname") {
        None => DEFAULT_NICKNAME.to_string(),
        Some(Value::String(name)) if !name.trim().is_empty() => name.clone(),
        Some(Value::String(_)) => DEFAULT_NICKNAME.to_string(),
        Some(_) => return Err("\"nickname\" must be a string".to_string()),
    };
    Ok(ReplayEntry {
        delay_ms,
        channel,
        nickname,
        uid: number(object, "uid")?,
        text,
        class_id: number(object, "class_id")?.unwrap_or(0),
        level: number(object, "level")?.unwrap_or(0),
        timestamp: number(object, "timestamp")?,
        sequence_id: number(object, "sequence_id")?,
    })
}

fn number(object: &Map<String, Value>, name: &str) -> Result<Option<u64>, String> {
    match object.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("\"{name}\" must be a whole number, 0 or more")),
    }
}

fn channel_named(name: &str) -> Result<Channel, String> {
    let wanted = name.trim();
    Channel::ALL
        .into_iter()
        .find(|channel| channel.as_str().eq_ignore_ascii_case(wanted) || channel.label() == wanted)
        .ok_or_else(|| {
            let names: Vec<&str> = Channel::ALL.iter().map(|c| c.as_str()).collect();
            format!(
                "unknown channel {name:?} (use {}, or the tab's Korean label)",
                names.join(", ")
            )
        })
}

impl ReplayEntry {
    /// The message as the capture would have decoded it; the pipeline gives it
    /// its pid. `index` is the entry's place in the replay, counted from 0.
    pub fn into_chat(&self, index: usize, now_secs: u64) -> ChatMessage {
        ChatMessage {
            channel: self.channel,
            nickname: self.nickname.clone(),
            message: self.text.clone(),
            timestamp: self.timestamp.unwrap_or(now_secs),
            uid: self.uid.unwrap_or_else(|| default_uid(&self.nickname)),
            class_id: self.class_id,
            level: self.level,
            sequence_id: self.sequence_id.unwrap_or(index as u64 + 1),
            ..ChatMessage::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> ReplayEntry {
        let mut entries = parse_replay(line).expect("parses");
        assert_eq!(entries.len(), 1);
        entries.remove(0)
    }

    fn error(text: &str) -> ReplayError {
        parse_replay(text).expect_err("is refused")
    }

    #[test]
    fn a_line_with_only_text_gets_the_defaults() {
        assert_eq!(
            one(r#"{"text": "こんにちは"}"#),
            ReplayEntry {
                delay_ms: 0,
                channel: Channel::World,
                nickname: "Tester".into(),
                uid: None,
                text: "こんにちは".into(),
                class_id: 0,
                level: 0,
                timestamp: None,
                sequence_id: None,
            }
        );
    }

    #[test]
    fn every_field_is_read() {
        let entry = one(
            r#"{"text": "hi", "delay_ms": 250, "channel": "guild", "nickname": "ミナト", "uid": 77,
                "class_id": 3, "level": 60, "timestamp": 1700000000, "sequence_id": 9}"#
                .replace('\n', " ")
                .as_str(),
        );
        assert_eq!(entry.delay_ms, 250);
        assert_eq!(entry.channel, Channel::Guild);
        assert_eq!(entry.nickname, "ミナト");
        assert_eq!(entry.uid, Some(77));
        assert_eq!((entry.class_id, entry.level), (3, 60));
        assert_eq!(entry.timestamp, Some(1_700_000_000));
        assert_eq!(entry.sequence_id, Some(9));
    }

    #[test]
    fn a_channel_is_its_name_in_any_case_or_its_korean_label() {
        for (given, want) in [
            ("WORLD", Channel::World),
            ("party", Channel::Party),
            ("Local", Channel::Local),
            ("BEGINNER", Channel::Beginner),
            ("길드", Channel::Guild),
            ("월드", Channel::World),
        ] {
            let line = format!(r#"{{"text": "x", "channel": "{given}"}}"#);
            assert_eq!(one(&line).channel, want, "{given}");
        }
    }

    #[test]
    fn blank_lines_and_comments_are_skipped_but_still_counted() {
        let text = "# a comment\n\n{\"text\": \"a\"}\n   \n{\"text\": \"b\"}\n{oops\n";
        let err = error(text);
        assert_eq!(err.line, 6, "{err}");
        let ok = parse_replay("# c\n\n{\"text\": \"a\"}\n\n{\"text\": \"b\"}\n").unwrap();
        assert_eq!(ok.len(), 2);
    }

    #[test]
    fn an_empty_file_has_no_entries() {
        assert_eq!(parse_replay("").unwrap(), vec![]);
        assert_eq!(parse_replay("# nothing\n\n").unwrap(), vec![]);
    }

    #[test]
    fn mistakes_are_errors_with_the_line_and_a_reason() {
        let cases = [
            ("not json", "not valid JSON"),
            ("[1, 2]", "not a JSON object"),
            (r#"{"delay_ms": 5}"#, "\"text\" is missing"),
            (r#"{"text": ""}"#, "\"text\" is empty"),
            (r#"{"text": "  "}"#, "\"text\" is empty"),
            (r#"{"text": 5}"#, "\"text\" must be a string"),
            (r#"{"text": "x", "delay": 5}"#, "unknown field \"delay\""),
            (
                r#"{"text": "x", "delay_ms": -1}"#,
                "\"delay_ms\" must be a whole number",
            ),
            (
                r#"{"text": "x", "delay_ms": "5"}"#,
                "\"delay_ms\" must be a whole number",
            ),
            (
                r#"{"text": "x", "delay_ms": 1.5}"#,
                "\"delay_ms\" must be a whole number",
            ),
            (r#"{"text": "x", "delay_ms": 600001}"#, "over 600000"),
            (
                r#"{"text": "x", "channel": "TRADE"}"#,
                "unknown channel \"TRADE\"",
            ),
            (
                r#"{"text": "x", "channel": 3}"#,
                "\"channel\" must be a string",
            ),
            (
                r#"{"text": "x", "nickname": 3}"#,
                "\"nickname\" must be a string",
            ),
            (
                r#"{"text": "x", "uid": "7"}"#,
                "\"uid\" must be a whole number",
            ),
        ];
        for (line, reason) in cases {
            let err = error(line);
            assert_eq!(err.line, 1, "{line}");
            assert!(err.reason.contains(reason), "{line}: {err}");
        }
    }

    #[test]
    fn the_error_names_the_line() {
        let err = error("{\"text\": \"ok\"}\n{\"text\": \"x\", \"chanel\": \"WORLD\"}\n");
        assert_eq!(err.to_string().split(':').next(), Some("line 2"));
        assert!(err.to_string().contains("unknown field \"chanel\""));
    }

    #[test]
    fn a_chat_takes_the_given_values_and_leaves_the_pid_to_the_pipeline() {
        let entry = one(
            r#"{"text": "hi", "channel": "PARTY", "nickname": "Kenji", "uid": 5, "class_id": 2, "level": 40, "timestamp": 123, "sequence_id": 8}"#,
        );
        let chat = entry.into_chat(3, 999);
        assert_eq!(chat.pid, 0);
        assert_eq!(
            (chat.channel, chat.nickname.as_str(), chat.message.as_str()),
            (Channel::Party, "Kenji", "hi")
        );
        assert_eq!((chat.uid, chat.class_id, chat.level), (5, 2, 40));
        assert_eq!((chat.timestamp, chat.sequence_id), (123, 8));
        assert!(!chat.is_blocked && chat.translated.is_none());
    }

    #[test]
    fn a_chat_without_time_or_sequence_is_live_and_unique() {
        let entries = parse_replay("{\"text\": \"a\"}\n{\"text\": \"a\"}\n").unwrap();
        let first = entries[0].into_chat(0, 1_700_000_000);
        let second = entries[1].into_chat(1, 1_700_000_000);
        assert_eq!(first.timestamp, 1_700_000_000);
        assert_eq!((first.sequence_id, second.sequence_id), (1, 2));
        // the same sender, the same second, the same words: still two messages
        assert_ne!(
            (first.uid, first.timestamp, first.sequence_id),
            (second.uid, second.timestamp, second.sequence_id)
        );
    }

    #[test]
    fn the_default_uid_is_stable_per_nickname_and_small() {
        let a = default_uid("ミナト");
        assert_eq!(a, default_uid("ミナト"));
        assert_ne!(a, default_uid("Kenji"));
        for name in ["", "a", "ミナト", "Kenji", "ゆう", "Tester"] {
            let id = default_uid(name);
            assert!(id > 0 && id < (1 << 52), "{name}: {id}");
        }
        let chat = one(r#"{"text": "x", "nickname": "ミナト"}"#).into_chat(0, 1);
        assert_eq!(chat.uid, a);
    }

    #[test]
    fn the_shipped_sample_parses() {
        let sample = include_str!("../testdata/replay-sample.jsonl");
        let entries = parse_replay(sample).expect("the sample is valid");
        assert!(entries.len() >= 6);
        assert!(entries.iter().any(|e| e.channel == Channel::Guild));
        assert!(entries.iter().any(|e| e.text.contains("<sprite=")));
        assert!(entries.iter().all(|e| e.delay_ms <= MAX_DELAY_MS));
    }

    /// The app hides WORLD chat from senders below its minimum level (default 1,
    /// `min_sender_level`), so a sample whose WORLD lines say level 0 shows no
    /// world chat at all -- what a first Windows run saw.
    #[test]
    fn the_shipped_samples_world_lines_get_past_the_default_level_filter() {
        let sample = include_str!("../testdata/replay-sample.jsonl");
        let entries = parse_replay(sample).expect("the sample is valid");
        let world: Vec<_> = entries
            .iter()
            .filter(|e| e.channel == Channel::World)
            .collect();
        assert!(!world.is_empty());
        assert!(
            world.iter().all(|e| e.level >= 1),
            "a WORLD line with level 0"
        );
    }
}
