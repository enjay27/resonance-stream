//! The test bridge's wire rules: which MQTT topics the app uses, how an event
//! is wrapped, and which commands a test may send. Pure -- the app's client
//! (`src-tauri/src/bridge.rs`) only moves these bytes.
//!
//! One broker, one app: the app publishes what it does under `rs/app/...` and
//! obeys what a test publishes under `rs/test/command/<name>`:
//!
//! | topic | from | payload |
//! |---|---|---|
//! | `rs/app/status` (retained) | app | `online` / `offline` (last will) |
//! | `rs/app/event/<name>` | app | [`envelope`] of a backend -> UI event |
//! | `rs/test/command/<name>` | test | `{"id": "...", ...arguments}` ([`parse_command`]) |
//! | `rs/app/ack/<id>` | app | `{"id","ok":true}` or `{"id","ok":false,"error"}` |
//! | `rs/app/error` | app | `{"error"}` for a message that had no usable id |
//!
//! A command is on an allowlist ([`Command`]): a test can ask for what the
//! flags already do (`replay-chat`), not run arbitrary code.

use serde_json::{json, Value};
use std::fmt;
use std::path::PathBuf;

/// Every backend -> UI event name the app emits. The bridge listens to each.
pub const EVENT_NAMES: [&str; 13] = [
    "chat-message-update",
    "sniffer-state",
    "translator-state",
    "translation-event",
    "system-event",
    "packet-event",
    "favorites-changed",
    "firewall-missing",
    "download-progress",
    "popup-shown",
    "global-tab-switch",
    "tray-toggle-always-on-top",
    "tray-toggle-click-through",
];

pub const STATUS_TOPIC: &str = "rs/app/status";
pub const ERROR_TOPIC: &str = "rs/app/error";
/// What the app subscribes to for commands.
pub const COMMAND_FILTER: &str = "rs/test/command/+";
const COMMAND_PREFIX: &str = "rs/test/command/";

pub fn event_topic(name: &str) -> String {
    format!("rs/app/event/{name}")
}

pub fn ack_topic(id: &str) -> String {
    format!("rs/app/ack/{id}")
}

/// One event as published: a counter, the time, the event name and its payload.
pub fn envelope(seq: u64, t_ms: u64, name: &str, payload: Value) -> String {
    json!({ "seq": seq, "t_ms": t_ms, "name": name, "payload": payload }).to_string()
}

/// What a test may ask the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Only answers: is the app there and listening?
    Ping,
    /// Close the app.
    Quit,
    /// Feed the chat lines of this file in, as `--replay-chat` does.
    ReplayChat { path: PathBuf },
}

/// A command and the id its ack carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// The topic is not `rs/test/command/<name>`.
    NotACommandTopic(String),
    /// The payload is not a JSON object.
    BadPayload,
    /// No usable `id` (a non-empty string without `/`, `+`, `#`).
    BadId,
    /// `name` is not on the allowlist. Carries the id, to ack with.
    Unknown { id: String, name: String },
    /// An argument is missing or has the wrong type. Carries the id.
    BadArgument { id: String, reason: String },
}

impl CommandError {
    /// The id the ack goes to, when the request got far enough to have one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Unknown { id, .. } | Self::BadArgument { id, .. } => Some(id),
            _ => None,
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotACommandTopic(t) => write!(f, "{t:?} is not a command topic"),
            Self::BadPayload => write!(f, "the payload is not a JSON object"),
            Self::BadId => write!(f, "the payload needs a string \"id\" (no / + #)"),
            Self::Unknown { name, .. } => write!(f, "unknown command {name:?}"),
            Self::BadArgument { reason, .. } => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Reads a command published on `topic` with `payload`.
pub fn parse_command(topic: &str, payload: &[u8]) -> Result<Request, CommandError> {
    let name = topic
        .strip_prefix(COMMAND_PREFIX)
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .ok_or_else(|| CommandError::NotACommandTopic(topic.to_string()))?;
    let Ok(Value::Object(args)) = serde_json::from_slice::<Value>(payload) else {
        return Err(CommandError::BadPayload);
    };
    let id = match args.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() && !id.contains(['/', '+', '#']) => id.to_string(),
        _ => return Err(CommandError::BadId),
    };
    let command = match name {
        "ping" => Command::Ping,
        "quit" => Command::Quit,
        "replay-chat" => match args.get("path").and_then(Value::as_str) {
            Some(path) if !path.is_empty() => Command::ReplayChat { path: path.into() },
            _ => {
                return Err(CommandError::BadArgument {
                    id,
                    reason: "replay-chat needs a string \"path\"".into(),
                })
            }
        },
        _ => {
            return Err(CommandError::Unknown {
                id,
                name: name.to_string(),
            })
        }
    };
    Ok(Request { id, command })
}

/// The ack of a request: ok, or why not.
pub fn ack(id: &str, result: Result<(), &str>) -> String {
    match result {
        Ok(()) => json!({ "id": id, "ok": true }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    }
    .to_string()
}

/// The payload for [`ERROR_TOPIC`].
pub fn error_message(error: &CommandError) -> String {
    match error.id() {
        Some(id) => json!({ "id": id, "error": error.to_string() }),
        None => json!({ "error": error.to_string() }),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(name: &str, payload: &str) -> Result<Request, CommandError> {
        parse_command(&format!("rs/test/command/{name}"), payload.as_bytes())
    }

    #[test]
    fn topics_are_under_rs_app() {
        assert_eq!(
            event_topic("chat-message-update"),
            "rs/app/event/chat-message-update"
        );
        assert_eq!(ack_topic("a1"), "rs/app/ack/a1");
    }

    #[test]
    fn every_event_name_is_one_topic_level() {
        for name in EVENT_NAMES {
            assert!(
                !name.is_empty() && !name.contains(['/', '+', '#']),
                "{name}"
            );
        }
        let mut sorted = EVENT_NAMES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), EVENT_NAMES.len(), "a name is listed twice");
    }

    #[test]
    fn an_envelope_carries_seq_time_name_and_payload() {
        let text = envelope(7, 1234, "sniffer-state", json!({"running": true}));
        let v: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(
            v,
            json!({"seq": 7, "t_ms": 1234, "name": "sniffer-state", "payload": {"running": true}})
        );
    }

    #[test]
    fn the_allowed_commands_parse() {
        assert_eq!(
            parse("ping", r#"{"id":"a1"}"#),
            Ok(Request {
                id: "a1".into(),
                command: Command::Ping
            })
        );
        assert_eq!(
            parse("quit", r#"{"id":"a2"}"#).map(|r| r.command),
            Ok(Command::Quit)
        );
        assert_eq!(
            parse("replay-chat", r#"{"id":"a3","path":"C:\\w\\chat.jsonl"}"#),
            Ok(Request {
                id: "a3".into(),
                command: Command::ReplayChat {
                    path: "C:\\w\\chat.jsonl".into()
                }
            })
        );
    }

    #[test]
    fn a_topic_that_is_not_a_command_is_refused() {
        for topic in [
            "rs/app/event/ping",
            "rs/test/command/",
            "rs/test/command/a/b",
            "rs/test/other/ping",
        ] {
            assert_eq!(
                parse_command(topic, br#"{"id":"x"}"#),
                Err(CommandError::NotACommandTopic(topic.into())),
                "{topic}"
            );
        }
    }

    #[test]
    fn a_payload_without_a_usable_id_is_refused() {
        assert_eq!(parse("ping", "not json"), Err(CommandError::BadPayload));
        assert_eq!(parse("ping", "[1]"), Err(CommandError::BadPayload));
        for payload in [
            "{}",
            r#"{"id":""}"#,
            r#"{"id":5}"#,
            r#"{"id":"a/b"}"#,
            r#"{"id":"a+"}"#,
            r##"{"id":"#"}"##,
        ] {
            assert_eq!(
                parse("ping", payload),
                Err(CommandError::BadId),
                "{payload}"
            );
        }
    }

    #[test]
    fn an_unknown_command_keeps_its_id_for_the_ack() {
        let err = parse("rm-rf", r#"{"id":"z"}"#).unwrap_err();
        assert_eq!(
            err,
            CommandError::Unknown {
                id: "z".into(),
                name: "rm-rf".into()
            }
        );
        assert_eq!(err.id(), Some("z"));
        assert_eq!(CommandError::BadId.id(), None);
    }

    #[test]
    fn replay_chat_needs_a_path() {
        for payload in [
            r#"{"id":"r"}"#,
            r#"{"id":"r","path":""}"#,
            r#"{"id":"r","path":3}"#,
        ] {
            let err = parse("replay-chat", payload).unwrap_err();
            assert!(
                matches!(&err, CommandError::BadArgument { id, .. } if id == "r"),
                "{payload}: {err:?}"
            );
        }
    }

    #[test]
    fn acks_and_errors_are_json() {
        let ok: Value = serde_json::from_str(&ack("a1", Ok(()))).expect("json");
        assert_eq!(ok, json!({"id": "a1", "ok": true}));
        let bad: Value = serde_json::from_str(&ack("a1", Err("no file"))).expect("json");
        assert_eq!(bad, json!({"id": "a1", "ok": false, "error": "no file"}));
        let err: Value =
            serde_json::from_str(&error_message(&CommandError::BadPayload)).expect("json");
        assert_eq!(err, json!({"error": "the payload is not a JSON object"}));
        let with_id: Value = serde_json::from_str(&error_message(&CommandError::Unknown {
            id: "z".into(),
            name: "x".into(),
        }))
        .expect("json");
        assert_eq!(with_id["id"], "z");
    }
}
