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
    #[serde(default)]
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
