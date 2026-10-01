use crate::{inject_system_message, AppState, SystemLogLevel, TranslatorState};
use resonance_core::download::write_atomic;
use resonance_core::history::ChannelLimits;
use resonance_core::workers::{translator_change, TranslatorSettings, WorkerChange};
use resonance_types::{
    default_catch_up_limit, default_favorite_messages, Channel, ComputeMode, FavoriteMessage,
    LogLevel, TabSwitchModifier, Theme, Tier, ALL_TAB, CUSTOM_TAB,
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DisplayFromStr;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

/// `#[serde(default)]`: a field missing from the file (an older version, a
/// hand edit) takes its default instead of failing the whole file.
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub init_done: bool,
    pub use_translation: bool,
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
    /// Japanese messages missed since the app opened that a translator start
    /// translates (the newest ones); 0 turns the catch-up off.
    #[serde(default = "default_catch_up_limit")]
    pub translation_catch_up_limit: usize,
    pub hide_original_in_compact: bool,
    pub network_interface: String,
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
    /// Messages each tab keeps (keys: channel names, `ALL_TAB`, `CUSTOM_TAB`).
    #[serde(default = "default_tab_limits")]
    pub tab_limits: std::collections::HashMap<String, usize>,
    /// Channels not written to the chat archive.
    #[serde(default = "default_archive_ignored_channels")]
    pub archive_ignored_channels: Vec<String>,
    #[serde(default = "default_spacing")]
    pub message_spacing: u32,
    /// Chat lines to copy or paste by shortcut; see `shortcut.rs`.
    #[serde(default = "default_favorite_messages")]
    pub favorite_messages: Vec<FavoriteMessage>,
    /// Names of the favorites tabs the user made (the default tab is not
    /// listed); each message carries its own tab name.
    #[serde(default)]
    pub favorite_tabs: Vec<String>,
    /// Days of daily chat logs (chat_logs/) to keep; 0 keeps them all.
    #[serde(default)]
    pub chat_log_retention_days: u32,
    /// Debug: append the game's raw port-5003 packets to `captures/`. Applies live.
    #[serde(default)]
    pub raw_capture: bool,
}

fn default_spacing() -> u32 {
    4
}

fn default_tab_limits() -> std::collections::HashMap<String, usize> {
    [
        (Channel::World.as_str(), 200), // World gets a small limit
        (Channel::Local.as_str(), 500),
        (Channel::Party.as_str(), 1000), // Party/Guild get huge limits
        (Channel::Guild.as_str(), 1000),
        (Channel::Beginner.as_str(), 500),
        (ALL_TAB, 1000),
        (CUSTOM_TAB, 1000),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

fn default_archive_ignored_channels() -> Vec<String> {
    vec![Channel::World.as_str().to_string()]
}

impl AppConfig {
    /// Messages the backend keeps (and reloads) per channel: the UI's
    /// channel-tab limits (see src/chat_view.rs tab_limit).
    pub fn channel_limits(&self) -> ChannelLimits {
        ChannelLimits::new(&self.tab_limits)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            init_done: false,
            use_translation: false,
            compute_mode: ComputeMode::default(),
            compact_mode: false,
            always_on_top: false,
            active_tab: ALL_TAB.to_string(),
            custom_tab_filters: Channel::ALL.map(|c| c.as_str().to_string()).to_vec(),
            theme: Theme::default(),
            overlay_opacity: 0.85,
            debug_mode: false,
            log_level: LogLevel::default(),
            tier: Tier::default(),
            translation_catch_up_limit: default_catch_up_limit(),
            hide_original_in_compact: false,
            network_interface: "".to_string(),
            drag_to_scroll: false,
            alert_keywords: vec![],
            alert_volume: 0.5,
            emphasis_keywords: vec![],
            use_relative_time: false,
            font_size: 14,
            hide_blocked_messages: false,
            blocked_users: std::collections::HashMap::new(),
            min_sender_level: 1,
            auto_sync_latest_dict: false,
            tab_switch_modifier: TabSwitchModifier::default(),
            tab_switch_key: "Tab".to_string(),
            tab_limits: default_tab_limits(),
            archive_ignored_channels: default_archive_ignored_channels(),
            message_spacing: default_spacing(),
            favorite_messages: default_favorite_messages(),
            favorite_tabs: Vec::new(),
            chat_log_retention_days: 0,
            raw_capture: false,
        }
    }
}

fn get_config_path(app: &AppHandle) -> PathBuf {
    let config_dir = app
        .path()
        .app_config_dir()
        .expect("Could not resolve app config dir");

    // Ensure the directory exists (e.g., create 'com.bpsr.translator' folder)
    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }

    config_dir.join("config.json")
}

/// Reads `config.json`, writing the defaults first if it does not exist yet.
/// Only start-up (and early callers before the state exists) read the file.
pub fn read_config_file(app: &AppHandle) -> AppConfig {
    let path = get_config_path(app);

    if !path.exists() {
        // Create default if missing
        let default_config = AppConfig::default();
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            let _ = write_atomic(&path, json.as_bytes());
        }
        return default_config;
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
            // Keep the unreadable file: the next save would overwrite it.
            let backup = path.with_extension("json.bad");
            let _ = fs::copy(&path, &backup);
            log::error!("config.json unreadable ({e}); defaults used, file kept as {backup:?}");
            AppConfig::default()
        }),
        Err(_) => AppConfig::default(),
    }
}

fn translator_settings(c: &AppConfig) -> TranslatorSettings {
    TranslatorSettings {
        enabled: c.use_translation,
        compute_mode: c.compute_mode,
        tier: c.tier,
    }
}

/// The live config: the in-memory copy once the app state exists.
pub fn current_config(app: &AppHandle) -> AppConfig {
    match app.try_state::<AppState>() {
        Some(state) => state.config.read().clone(),
        None => read_config_file(app),
    }
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> AppConfig {
    current_config(&app)
}

/// async: writes the file and may start or stop workers -- not on the main thread.
#[tauri::command(async)]
pub fn save_config(app: AppHandle, state: State<'_, AppState>, config: AppConfig) {
    // One save at a time: two overlapping saves would each compare against
    // the same old config and start (or stop) the same worker twice.
    let _saving = state.config_lock.lock();
    apply_config(&app, &state, config);
}

/// Read-modify-write of the live config under the save lock, so a change
/// made from the backend (block list) cannot be lost to a concurrent save.
pub fn modify_config(
    app: &AppHandle,
    state: &State<'_, AppState>,
    change: impl FnOnce(&mut AppConfig),
) {
    let _saving = state.config_lock.lock();
    let mut config = state.config.read().clone();
    change(&mut config);
    apply_config(app, state, config);
}

fn apply_config(app: &AppHandle, state: &State<'_, AppState>, config: AppConfig) {
    let app = app.clone();
    let old_config = state.config.read().clone();

    let path = get_config_path(&app);
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        if let Err(e) = write_atomic(&path, json.as_bytes()) {
            log::error!("config.json not saved: {e}");
        }
    }
    *state.config.write() = config.clone();
    state
        .chat_history
        .lock()
        .set_limits(config.channel_limits());

    if old_config.favorite_messages != config.favorite_messages {
        state.shortcuts.lock().favorites = config.favorite_messages.clone();
        crate::shortcut::apply_global_shortcuts(&app);
    }

    // --- MANAGE THE SNIFFER THREAD (NETWORK ADAPTER CHANGE) ---
    if old_config.network_interface != config.network_interface {
        // Drop the old Sender (Instantly kills the socket and watchdog threads)
        *state.sniffer_tx.lock() = None;

        // Restart the sniffer bound to the newly selected interface
        if config.init_done {
            inject_system_message(
                &app,
                SystemLogLevel::Info,
                "Sniffer",
                "Network adapter changed. Restarting sniffer...",
            );
            let tx = crate::services::sniffer::start_sniffer_worker(app.clone());
            *state.sniffer_tx.lock() = Some(tx);
        }
    }

    // --- MANAGE THE AI WORKER THREAD --- (exactly one change per save)
    match translator_change(
        translator_settings(&old_config),
        translator_settings(&config),
    ) {
        WorkerChange::Keep => {}
        change @ (WorkerChange::Start | WorkerChange::Restart) => {
            if change == WorkerChange::Restart {
                // Drop the old sender to break the current thread's loop
                *state.translator_tx.lock() = None;
                inject_system_message(
                    &app,
                    SystemLogLevel::Info,
                    "Translator",
                    "Applying new AI Engine specifications...",
                );
            }
            let model_path = crate::get_model_path(&app);
            let tx = crate::services::translator::start_translator_worker(app.clone(), model_path);
            *state.translator_tx.lock() = Some(tx);
        }
        WorkerChange::Stop => {
            // Drop the Sender (Kills the thread and frees VRAM)
            *state.translator_tx.lock() = None;
            crate::services::translator::retire_translator_workers();
            inject_system_message(
                &app,
                SystemLogLevel::Info,
                "Translator",
                "AI Translation Disabled. Server stopped and VRAM cleared.",
            );
            crate::services::translator::emit_translator_state(
                &app,
                TranslatorState::Off,
                "AI Translation Disabled.",
            );
        }
    }

    if old_config.raw_capture != config.raw_capture {
        crate::services::sniffer::set_raw_capture(config.raw_capture);
    }

    if old_config.chat_log_retention_days != config.chat_log_retention_days {
        crate::io::prune_chat_logs(&app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_missing_fields_keeps_the_rest() {
        // Regression (N3): one missing field used to reset every setting.
        let config: AppConfig = serde_json::from_str(
            r#"{"init_done": true, "theme": "light", "blocked_users": {"7": "x"}}"#,
        )
        .unwrap();
        assert!(config.init_done);
        assert_eq!(config.theme, Theme::Light);
        assert_eq!(config.blocked_users.get(&7).map(String::as_str), Some("x"));
        assert_eq!(config.font_size, 14); // absent: default
    }

    #[test]
    fn raw_capture_is_off_by_default() {
        assert!(!AppConfig::default().raw_capture);
        let config: AppConfig = serde_json::from_str(r#"{"init_done": true}"#).unwrap();
        assert!(!config.raw_capture);
    }

    #[test]
    fn favorite_tabs_are_empty_when_missing_and_round_trip() {
        assert!(AppConfig::default().favorite_tabs.is_empty());
        let old: AppConfig = serde_json::from_str(r#"{"init_done": true}"#).unwrap();
        assert!(old.favorite_tabs.is_empty());
        let config: AppConfig = serde_json::from_str(
            r#"{"favorite_tabs": ["레이드"], "favorite_messages": [{"text": "hi", "tab": "레이드"}]}"#,
        )
        .unwrap();
        assert_eq!(config.favorite_tabs, ["레이드"]);
        assert_eq!(config.favorite_messages[0].tab, "레이드");
        let saved = serde_json::to_value(&config).unwrap();
        assert_eq!(saved["favorite_tabs"][0], "레이드");
    }

    #[test]
    fn odd_setting_values_still_load() {
        // Hand-edited or from another version: case is ignored, a value
        // nobody knows is the default -- the rest of the file survives.
        let config: AppConfig = serde_json::from_str(
            r#"{"init_done": true, "compute_mode": "GPU", "tier": "extreme", "theme": "neon"}"#,
        )
        .unwrap();
        assert!(config.init_done);
        assert_eq!(config.compute_mode, ComputeMode::Gpu);
        assert_eq!(config.tier, Tier::Middle);
        assert_eq!(config.theme, Theme::Dark);
    }

    #[test]
    fn the_tab_switch_modifier_is_ctrl_when_missing_or_empty() {
        // A config saved before the shortcut existed has no modifier (and no
        // key, so nothing is registered); the ui always showed that as Ctrl.
        // The backend used to read the missing value as "no modifier".
        for json in [
            r#"{"init_done": true}"#,
            r#"{"tab_switch_modifier": ""}"#,
            r#"{"tab_switch_modifier": "Ctrl", "tab_switch_key": "Tab"}"#,
        ] {
            let config: AppConfig = serde_json::from_str(json).unwrap();
            assert_eq!(
                config.tab_switch_modifier,
                TabSwitchModifier::Ctrl,
                "{json}"
            );
        }
        let bare: AppConfig = serde_json::from_str(r#"{"tab_switch_modifier": "None"}"#).unwrap();
        assert_eq!(bare.tab_switch_modifier, TabSwitchModifier::NoModifier);
    }

    #[test]
    fn dictionary_auto_sync_is_off_by_default() {
        assert!(!AppConfig::default().auto_sync_latest_dict);
        // A config file without the field (older versions) reads as off too.
        let mut json = serde_json::to_value(AppConfig::default()).unwrap();
        json.as_object_mut()
            .unwrap()
            .remove("auto_sync_latest_dict");
        let config: AppConfig = serde_json::from_value(json).unwrap();
        assert!(!config.auto_sync_latest_dict);
    }
}
