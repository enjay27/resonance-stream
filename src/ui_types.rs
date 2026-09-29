use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DisplayFromStr;

// Types shared with the backend live in crates/types.
pub use resonance_types::{
    ChatMessage, FolderStatus, GistMetadata, NetworkInterface, ProgressPayload, RemoteDictionary,
    SnifferStatePayload, SystemMessage, TranslationResult, TranslatorStatePayload,
    UpdateCheckResult, VersionInfo,
};

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppConfig {
    pub init_done: bool,
    pub use_translation: bool,
    pub compute_mode: String,
    pub compact_mode: bool,
    pub always_on_top: bool,
    pub active_tab: String,
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
    #[serde(default)]
    pub tab_limits: std::collections::HashMap<String, usize>,
    #[serde(default)]
    pub archive_ignored_channels: Vec<String>,
    #[serde(default = "default_spacing")]
    pub message_spacing: u32,
}

pub fn default_spacing() -> u32 {
    4
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TauriEvent {
    pub payload: ProgressPayload,
}
