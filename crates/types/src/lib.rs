//! Types that cross the Tauri boundary: the backend serializes them, the UI
//! deserializes them (and some go back the other way). One definition, so the
//! two sides cannot drift apart. Field names and serde attributes ARE the wire
//! format — renaming one is a protocol change.
//!
//! Dependencies stay minimal (serde only): this crate compiles to wasm for the UI.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Chat and system log ---

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub pid: u64,
    pub channel: String,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnifferStatePayload {
    pub state: String,   // "Starting", "Firewall", "Binding", "Active", "Error", "Off"
    pub message: String, // Context or Error message
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TranslatorStatePayload {
    pub state: String, // "Starting", "Loading Model", "Active", "Error", "Off"
    pub message: String,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn non_empty_unknown_fields_still_cross() {
        let mut msg = ChatMessage::default();
        msg.unknown_fields.insert("chat_40".into(), vec![7]);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["unknownFields"]["chat_40"][0], 7);
    }
}
