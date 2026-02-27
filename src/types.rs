use serde::{Deserialize, Serialize};

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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FolderStatus {
    pub exists: bool, 
    pub path: String 
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TauriEvent { 
    pub payload: ProgressPayload 
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
pub struct SnifferStatePayload {
    pub state: String,   // "Starting", "Firewall", "Binding", "Active", "Error", "Off"
    pub message: String, // Context or Error message
}