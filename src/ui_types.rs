use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DisplayFromStr;

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
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemMessage {
    pub pid: u64,
    pub timestamp: u64,
    pub level: String,  // info, warn, error, success, debug
    pub source: String, // Backend, Sniffer, Sidecar
    pub message: String,
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppConfig {
    pub init_done: bool,
    pub use_translation: bool,
    pub compute_mode: String,
    pub compact_mode: bool,
    pub always_on_top: bool,
    pub active_tab: String,
    pub chat_limit: usize,
    pub custom_tab_filters: Vec<String>,
    pub theme: String,
    pub overlay_opacity: f32,
    pub debug_mode: bool,
    pub log_level: String,
    pub tier: String,
    pub archive_chat: bool,
    pub hide_original_in_compact: bool,
    #[serde(default)]
    pub network_interface: String,
    #[serde(default)]
    pub drag_to_scroll: bool,
    pub alert_keywords: Vec<String>,
    pub alert_volume: f32,
    pub emphasis_keywords: Vec<String>,
    pub use_relative_time: bool,
    pub font_size: u32,
    #[serde(default)]
    pub hide_blocked_messages: bool,
    #[serde_as(as = "std::collections::HashMap<DisplayFromStr, _>")]
    pub blocked_users: std::collections::HashMap<u64, String>,
    #[serde(default)]
    pub min_sender_level: u64,
    #[serde(default)]
    pub auto_sync_latest_dict: bool,
    #[serde(default)]
    pub tab_switch_modifier: String, // e.g., "Ctrl", "Alt", "Shift"
    #[serde(default)]
    pub tab_switch_key: String, // e.g., "Tab", "ArrowRight", etc.
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FolderStatus {
    pub exists: bool,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TauriEvent {
    pub payload: ProgressPayload,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TranslationResult {
    pub pid: u64,
    pub translated: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProgressPayload {
    #[serde(rename = "current_file")] // Match backend field name
    pub current_file: String,
    pub percent: u8,
    #[serde(rename = "total_percent")] // Match backend field name
    pub total_percent: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct TranslatorStatePayload {
    pub state: String, // "Starting", "Loading Model", "Active", "Error", "Off"
    pub message: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SnifferStatePayload {
    pub state: String,   // "Starting", "Firewall", "Binding", "Active", "Error", "Off"
    pub message: String, // Context or Error message
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionInfo {
    pub latest_version: String,
    pub download_url: String,
    pub release_notes: String,
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
