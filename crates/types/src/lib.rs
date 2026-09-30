//! Types that cross the Tauri boundary: the backend serializes them, the UI
//! deserializes them (and some go back the other way). One definition, so the
//! two sides cannot drift apart. Field names and serde attributes ARE the wire
//! format — renaming one is a protocol change.
//!
//! Dependencies stay minimal (serde only): this crate compiles to wasm for the UI.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Chat and system log ---

/// A chat channel. On the wire and on disk it is its upper-case name
/// (`"WORLD"`, `"GUILD"`, ...); a name this enum does not know -- an old log, a
/// channel the game has that we do not show separately yet (the beginner
/// channel, code 9) -- reads as [`Channel::World`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    #[default]
    World,
    Local,
    Party,
    Guild,
}

impl Channel {
    /// Every channel, in the order tabs and menus list them.
    pub const ALL: [Channel; 4] = [
        Channel::World,
        Channel::Local,
        Channel::Party,
        Channel::Guild,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::World => "WORLD",
            Channel::Local => "LOCAL",
            Channel::Party => "PARTY",
            Channel::Guild => "GUILD",
        }
    }

    /// The channel called `name` (exact, upper case); unknown names are world.
    pub fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|channel| channel.as_str() == name)
            .unwrap_or_default()
    }

    /// The channel for the number the game sends in a chat frame: 2 local,
    /// 3 party, 4 guild; anything else (1 world, 9 beginner, ...) is world.
    pub fn from_code(code: u64) -> Self {
        match code {
            2 => Channel::Local,
            3 => Channel::Party,
            4 => Channel::Guild,
            _ => Channel::World,
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Channel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Channel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_name(&String::deserialize(deserializer)?))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatMessage {
    pub pid: u64,
    pub channel: Channel,
    pub nickname: String,
    pub message: String,
    pub timestamp: u64,
    pub uid: u64,
    pub class_id: u64,
    pub level: u64,
    pub sequence_id: u64,
    #[serde(default)]
    pub is_blocked: bool,
    // --- Translation Support ---
    #[serde(default)]
    pub translated: Option<String>,
    #[serde(default)]
    pub nickname_romaji: Option<String>,
    /// Raw bytes of fields the parser does not understand. Empty unless the
    /// backend runs with unknown-field capture on; omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub unknown_fields: HashMap<String, Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemMessage {
    pub pid: u64,        // Unique ID for Leptos 'For' loop keys
    pub timestamp: u64,  // Milliseconds for sorting
    pub level: String,   // "info", "warn", "error", "success", "debug"
    pub source: String,  // "Backend", "Sniffer", "Translator"
    pub message: String, // The actual log text
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TranslationResult {
    pub pid: u64,
    pub translated: String,
}

// --- Service state events ---

/// What the packet sniffer is doing. On the wire it is the variant's name
/// (`"Active"`); a name this enum does not know reads as `Off`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SnifferState {
    Starting,
    Binding,
    Pending,
    Active,
    Error,
    /// Also what any unknown state reads as.
    #[default]
    #[serde(other)]
    Off,
}

impl SnifferState {
    /// What the title-bar badge shows for a transitional state.
    pub fn label(self) -> String {
        match self {
            SnifferState::Starting => "STARTING",
            SnifferState::Binding => "BINDING",
            SnifferState::Pending => "PENDING",
            SnifferState::Active => "ACTIVE",
            SnifferState::Error => "ERROR",
            SnifferState::Off => "OFF",
        }
        .to_string()
    }
}

/// What the translator (llama-server and its worker) is doing; the wire form
/// is the name with spaces (`"Loading Model"`), unknown reads as `Off`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TranslatorState {
    Starting,
    #[serde(rename = "Loading Model")]
    LoadingModel,
    #[serde(rename = "Catching Up")]
    CatchingUp,
    Restarting,
    Active,
    Error,
    /// Also what any unknown state reads as.
    #[default]
    #[serde(other)]
    Off,
}

impl TranslatorState {
    /// What the title-bar badge shows for a transitional state.
    pub fn label(self) -> String {
        match self {
            TranslatorState::Starting => "STARTING",
            TranslatorState::LoadingModel => "LOADING MODEL",
            TranslatorState::CatchingUp => "CATCHING UP",
            TranslatorState::Restarting => "RESTARTING",
            TranslatorState::Active => "ACTIVE",
            TranslatorState::Error => "ERROR",
            TranslatorState::Off => "OFF",
        }
        .to_string()
    }
}

/// Orders state changes: every change of either service gets a larger `seq`,
/// so the UI can drop a snapshot (`get_service_states`) that an event
/// overtook. 0: no order known (a payload from before `seq` existed).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SnifferStatePayload {
    pub state: SnifferState,
    pub message: String, // Context or Error message
    #[serde(default)]
    pub seq: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TranslatorStatePayload {
    pub state: TranslatorState,
    pub message: String,
    #[serde(default)]
    pub seq: u64,
}

impl Default for SnifferStatePayload {
    fn default() -> Self {
        Self {
            state: SnifferState::Off,
            message: String::new(),
            seq: 0,
        }
    }
}

impl Default for TranslatorStatePayload {
    fn default() -> Self {
        Self {
            state: TranslatorState::Off,
            message: String::new(),
            seq: 0,
        }
    }
}

/// The last state each service reported. The backend keeps it so a UI that
/// started listening late (or reloaded) can ask for it instead of waiting
/// for the next event.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ServiceStates {
    pub sniffer: SnifferStatePayload,
    pub translator: TranslatorStatePayload,
    #[serde(skip)]
    last_seq: u64,
}

impl ServiceStates {
    fn next_seq(&mut self) -> u64 {
        self.last_seq += 1;
        self.last_seq
    }

    /// Records a sniffer state; returns the payload to emit.
    pub fn set_sniffer(&mut self, state: SnifferState, message: &str) -> SnifferStatePayload {
        self.sniffer = SnifferStatePayload {
            state,
            message: message.to_string(),
            seq: self.next_seq(),
        };
        self.sniffer.clone()
    }

    /// Records a translator state; returns the payload to emit.
    pub fn set_translator(
        &mut self,
        state: TranslatorState,
        message: &str,
    ) -> TranslatorStatePayload {
        self.translator = TranslatorStatePayload {
            state,
            message: message.to_string(),
            seq: self.next_seq(),
        };
        self.translator.clone()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
}

// --- Favorite messages ---

/// A saved chat line: copied from the favorites panel, or pasted into the
/// game by its global shortcut. `shortcut` is a Tauri accelerator built from
/// `KeyboardEvent.code` ("Ctrl+Shift+Digit1"); empty means none. `note` is
/// a reminder shown under the text (its meaning in Korean); never sent.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteMessage {
    pub text: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub shortcut: String,
}

/// Missed Japanese messages a translator start translates when the config
/// does not say (`translation_catch_up_limit`, both AppConfigs).
pub fn default_catch_up_limit() -> usize {
    100
}

/// What a fresh config starts with: common Japanese chat lines with their
/// Korean meaning, no shortcuts.
pub fn default_favorite_messages() -> Vec<FavoriteMessage> {
    [
        ("こんにちは！", "안녕하세요"),
        ("おはようございます！", "좋은 아침입니다"),
        ("よろしくお願いします！", "잘 부탁드립니다"),
        ("ありがとうございます！", "감사합니다"),
        ("お疲れ様でした！", "수고하셨습니다"),
        ("すみません！", "죄송합니다"),
        ("またね！", "또 봐요"),
    ]
    .into_iter()
    .map(|(text, note)| FavoriteMessage {
        text: text.to_string(),
        note: note.to_string(),
        shortcut: String::new(),
    })
    .collect()
}

// --- Downloads and updates ---

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FolderStatus {
    pub exists: bool,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProgressPayload {
    pub current_file: String,
    pub percent: u8,
    pub total_percent: u8,
}

/// One entry of the gist's metadata.json.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionInfo {
    pub latest_version: String,
    pub download_url: String,
    pub release_notes: String,
    /// SHA-256 (hex) of the file at `download_url`; downloads are verified
    /// against it. Empty when the gist entry has none.
    #[serde(default)]
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteDictionary {
    pub version: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GistMetadata {
    pub app: VersionInfo,
    pub model: VersionInfo,
    pub dictionary: RemoteDictionary,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateCheckResult {
    pub app_update_available: bool,
    pub model_update_available: bool,
    pub dict_update_available: bool,
    pub remote_data: GistMetadata,
}

// --- Rules both sides apply ---

/// Whether `text` has Japanese in it (kana or kanji): the backend translates
/// such a line, and the UI marks its original. One definition, so the two
/// cannot disagree.
pub fn contains_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c,
            '\u{3005}'                 // 々 iteration mark
            | '\u{3040}'..='\u{30FF}'  // hiragana, katakana
            | '\u{31F0}'..='\u{31FF}'  // katakana phonetic extensions
            | '\u{3400}'..='\u{4DBF}'  // CJK extension A
            | '\u{4E00}'..='\u{9FFF}'  // CJK unified ideographs
            | '\u{FF66}'..='\u{FF9F}'  // half-width katakana
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_line_from_an_older_version_still_parses() {
        // Regression (W3): a saved line without fields added later (classId,
        // level, sequenceId ...) was skipped, so the history vanished.
        let old =
            r#"{"pid":3,"channel":"WORLD","nickname":"Bob","message":"hi","timestamp":5,"uid":9}"#;
        let msg: ChatMessage = serde_json::from_str(old).expect("old line must parse");
        assert_eq!((msg.pid, msg.uid, msg.message.as_str()), (3, 9, "hi"));
        assert_eq!((msg.class_id, msg.level, msg.sequence_id), (0, 0, 0));
    }

    #[test]
    fn a_channel_is_its_upper_case_name_on_the_wire() {
        for (channel, name) in [
            (Channel::World, "WORLD"),
            (Channel::Local, "LOCAL"),
            (Channel::Party, "PARTY"),
            (Channel::Guild, "GUILD"),
        ] {
            assert_eq!(serde_json::to_value(channel).unwrap(), name);
            assert_eq!(
                serde_json::from_value::<Channel>(name.into()).unwrap(),
                channel
            );
            assert_eq!(channel.as_str(), name);
            assert_eq!(Channel::from_name(name), channel);
        }
    }

    #[test]
    fn a_channel_name_nobody_knows_reads_as_world() {
        // Old logs, and channels the parser has no variant for yet.
        for unknown in ["", "BEGINNER", "world", "9", "길드"] {
            assert_eq!(Channel::from_name(unknown), Channel::World, "{unknown:?}");
            let json = serde_json::Value::String(unknown.into());
            assert_eq!(
                serde_json::from_value::<Channel>(json).unwrap(),
                Channel::World
            );
        }
    }

    #[test]
    fn the_game_sends_channels_as_numbers() {
        // One table (was in the parser twice): 2 local, 3 party, 4 guild,
        // anything else -- 1 is world, 9 the beginner channel -- world.
        let got: Vec<_> = [1, 2, 3, 4, 9, 0, 1000].map(Channel::from_code).to_vec();
        assert_eq!(
            got,
            [
                Channel::World,
                Channel::Local,
                Channel::Party,
                Channel::Guild,
                Channel::World,
                Channel::World,
                Channel::World
            ]
        );
    }

    #[test]
    fn every_channel_is_listed_once() {
        assert_eq!(Channel::ALL.len(), 4);
        let names: Vec<_> = Channel::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(names, ["WORLD", "LOCAL", "PARTY", "GUILD"]);
        assert_eq!(Channel::default(), Channel::World);
        assert_eq!(Channel::Guild.to_string(), "GUILD");
    }

    #[test]
    fn a_chat_message_keeps_its_channel_string_on_the_wire() {
        let msg = ChatMessage {
            channel: Channel::Guild,
            ..Default::default()
        };
        assert_eq!(serde_json::to_value(&msg).unwrap()["channel"], "GUILD");
        let old = r#"{"pid":1,"channel":"PARTY","message":"x"}"#;
        assert_eq!(
            serde_json::from_str::<ChatMessage>(old).unwrap().channel,
            Channel::Party
        );
        // A line saved without a channel, or with one we never had, is WORLD.
        let none = r#"{"pid":1,"message":"x"}"#;
        assert_eq!(
            serde_json::from_str::<ChatMessage>(none).unwrap().channel,
            Channel::World
        );
    }

    #[test]
    fn empty_unknown_fields_are_left_off_the_wire() {
        let json = serde_json::to_value(ChatMessage {
            pid: 1,
            message: "hi".into(),
            ..Default::default()
        })
        .unwrap();
        assert!(json.get("unknownFields").is_none(), "{json}");
        // ...and the field still deserializes when absent.
        let back: ChatMessage = serde_json::from_value(json).unwrap();
        assert!(back.unknown_fields.is_empty());
    }

    #[test]
    fn version_info_carries_the_sha256() {
        let entry: VersionInfo = serde_json::from_str(
            r#"{"latest_version":"0.5.0","download_url":"https://x/y","release_notes":"","sha256":"ab"}"#,
        )
        .unwrap();
        assert_eq!(entry.sha256, "ab");

        // A missing hash parses as empty (the download then refuses to run),
        // so one bad entry does not break the whole update check.
        let without: VersionInfo = serde_json::from_str(
            r#"{"latest_version":"0.4.0","download_url":"https://x/y","release_notes":""}"#,
        )
        .unwrap();
        assert!(without.sha256.is_empty());
    }

    #[test]
    fn favorite_defaults_have_no_shortcut_and_shortcut_is_optional() {
        let defaults = default_favorite_messages();
        assert!(!defaults.is_empty());
        assert!(defaults.iter().all(|f| f.shortcut.is_empty()));
        assert!(defaults.iter().all(|f| !f.note.is_empty()));

        // A list saved before notes existed still loads.
        let fav: FavoriteMessage = serde_json::from_str(r#"{"text":"hi"}"#).unwrap();
        assert_eq!(fav.text, "hi");
        assert!(fav.note.is_empty());
        assert!(fav.shortcut.is_empty());
    }

    #[test]
    fn service_states_start_off() {
        let states = ServiceStates::default();
        assert_eq!(states.sniffer.state, SnifferState::Off);
        assert_eq!(states.translator.state, TranslatorState::Off);
        assert_eq!((states.sniffer.seq, states.translator.seq), (0, 0));
    }

    #[test]
    fn service_states_are_their_display_names_on_the_wire() {
        use serde_json::{from_value, json, to_value};
        for (state, name) in [
            (SnifferState::Off, "Off"),
            (SnifferState::Starting, "Starting"),
            (SnifferState::Binding, "Binding"),
            (SnifferState::Pending, "Pending"),
            (SnifferState::Active, "Active"),
            (SnifferState::Error, "Error"),
        ] {
            assert_eq!(to_value(state).unwrap(), name);
            assert_eq!(from_value::<SnifferState>(json!(name)).unwrap(), state);
            assert_eq!(state.label(), name.to_uppercase());
        }
        for (state, name) in [
            (TranslatorState::Off, "Off"),
            (TranslatorState::Starting, "Starting"),
            (TranslatorState::LoadingModel, "Loading Model"),
            (TranslatorState::CatchingUp, "Catching Up"),
            (TranslatorState::Restarting, "Restarting"),
            (TranslatorState::Active, "Active"),
            (TranslatorState::Error, "Error"),
        ] {
            assert_eq!(to_value(state).unwrap(), name);
            assert_eq!(from_value::<TranslatorState>(json!(name)).unwrap(), state);
            assert_eq!(state.label(), name.to_uppercase());
        }
    }

    #[test]
    fn a_state_nobody_knows_reads_as_off() {
        // An older or newer backend, a hand-made payload.
        let sniffer: SnifferStatePayload =
            serde_json::from_str(r#"{"state":"Firewall","message":"m"}"#).unwrap();
        assert_eq!(sniffer.state, SnifferState::Off);
        let translator: TranslatorStatePayload =
            serde_json::from_str(r#"{"state":"","message":"m"}"#).unwrap();
        assert_eq!(translator.state, TranslatorState::Off);
    }

    #[test]
    fn every_state_change_is_newer_than_the_last_across_services() {
        // Regression (A1): the UI had no way to ask for the current state, and
        // could not tell a snapshot from an event that raced it.
        let mut states = ServiceStates::default();
        let a = states.set_translator(TranslatorState::Starting, "init");
        let b = states.set_sniffer(SnifferState::Active, "listening");
        let c = states.set_translator(TranslatorState::Active, "ready");
        assert!(0 < a.seq && a.seq < b.seq && b.seq < c.seq);
        assert_eq!(states.translator, c);
        assert_eq!(states.sniffer, b);
    }

    #[test]
    fn state_payload_carries_seq_and_an_older_one_still_parses() {
        let json =
            serde_json::to_value(ServiceStates::default().set_sniffer(SnifferState::Error, "x"))
                .unwrap();
        assert_eq!(json["seq"], 1);
        let old: TranslatorStatePayload =
            serde_json::from_str(r#"{"state":"Active","message":"m"}"#).unwrap();
        assert_eq!(old.seq, 0);
    }

    #[test]
    fn japanese_is_kana_or_kanji() {
        assert!(contains_japanese("こんにちは"));
        assert!(contains_japanese("カタカナ"));
        assert!(contains_japanese("漢字"));
        assert!(!contains_japanese("hello 123"));
        assert!(!contains_japanese("안녕하세요"));
    }

    #[test]
    fn japanese_the_old_ranges_missed() {
        // B6: half-width katakana, the iteration mark, the phonetic
        // extensions, CJK Extension A and the end of the unified block.
        assert!(contains_japanese("ｱﾘｶﾞﾄｳ"));
        assert!(contains_japanese("々"));
        assert!(contains_japanese("ㇰ"));
        assert!(contains_japanese("㐀"));
        assert!(contains_japanese("\u{9FC0}"));
        // Full-width Latin and half-width Korean are not Japanese.
        assert!(!contains_japanese("ｗｗｗ ＡＢＣ"));
        assert!(!contains_japanese("ﾡ"));
    }

    #[test]
    fn non_empty_unknown_fields_still_cross() {
        let mut msg = ChatMessage::default();
        msg.unknown_fields.insert("chat_40".into(), vec![7]);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["unknownFields"]["chat_40"][0], 7);
    }
}
