use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DisplayFromStr;

// Types shared with the backend live in crates/types.
pub use resonance_types::{
    contains_japanese, default_catch_up_limit, default_favorite_messages, Channel, ChatMessage,
    ComputeMode, FavoriteMessage, FavoriteTab, FolderStatus, GistMetadata, LogLevel,
    NetworkInterface, ProgressPayload, RemoteDictionary, RubySpan, ServiceStates, SnifferState,
    SnifferStatePayload, SystemLogLevel, SystemMessage, TabSwitchModifier, Theme, Tier,
    TranslationResult, TranslationView, TranslatorState, TranslatorStatePayload, UpdateCheckResult,
    VersionInfo, WindowRect, ALL_TAB, CUSTOM_TAB, SYSTEM_TAB,
};

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppConfig {
    pub init_done: bool,
    pub use_translation: bool,
    #[serde(default)]
    pub translation_view: TranslationView,
    pub compute_mode: ComputeMode,
    pub compact_mode: bool,
    pub always_on_top: bool,
    pub active_tab: String,
    pub custom_tab_filters: Vec<String>,
    pub theme: Theme,
    pub overlay_opacity: f32,
    pub debug_mode: bool,
    pub log_level: LogLevel,
    pub tier: Tier,
    /// Missed Japanese messages a translator start translates; 0 = none.
    #[serde(default = "default_catch_up_limit")]
    pub translation_catch_up_limit: usize,
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
    pub tab_switch_modifier: TabSwitchModifier,
    #[serde(default)]
    pub tab_switch_key: String, // e.g., "Tab", "ArrowRight", etc.
    #[serde(default)]
    pub tab_limits: std::collections::HashMap<String, usize>,
    #[serde(default)]
    pub archive_ignored_channels: Vec<String>,
    #[serde(default = "default_spacing")]
    pub message_spacing: u32,
    #[serde(default = "default_favorite_messages")]
    pub favorite_messages: Vec<FavoriteMessage>,
    /// The favorites tabs the user made, in display order (the default tab,
    /// id 0, is not listed).
    #[serde(default)]
    pub favorite_tabs: Vec<FavoriteTab>,
    /// Days of daily chat logs to keep; 0 keeps them all.
    #[serde(default)]
    pub chat_log_retention_days: u32,
    #[serde(default)]
    pub raw_capture: bool,
}

pub fn default_spacing() -> u32 {
    4
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TauriEvent {
    pub payload: ProgressPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same file the app's `AppConfig` test reads (`src-tauri/src/config/app_config.rs`):
    /// every field set to a value that is not its default. If one side lacks a field, names
    /// it differently or types it differently, the round trip loses it and the two differ.
    const FULL: &str = include_str!("../crates/types/testdata/app_config_full.json");

    #[test]
    fn the_ui_config_round_trips_every_field_the_app_writes() {
        let want: serde_json::Value = serde_json::from_str(FULL).expect("fixture is JSON");
        let config: AppConfig =
            serde_json::from_str(FULL).expect("fixture reads as the ui's config");
        let got = serde_json::to_value(&config).expect("config serializes");
        assert_eq!(got, want);
    }
}
