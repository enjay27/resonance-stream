use crate::ui_types::{ChatMessage, SystemMessage};
use leptos::prelude::{signal, Action, ReadSignal, RwSignal, WriteSignal};
use std::collections::{BTreeMap, HashMap};

#[derive(Copy, Clone, Debug)]
pub struct AppSignals {
    // Add other global controls here
    pub init_done: ReadSignal<bool>,
    pub set_init_done: WriteSignal<bool>,
    pub use_translation: ReadSignal<bool>,
    pub set_use_translation: WriteSignal<bool>,
    pub compute_mode: ReadSignal<String>,
    pub set_compute_mode: WriteSignal<String>,
    pub wizard_step: ReadSignal<i32>,
    pub set_wizard_step: WriteSignal<i32>,
    pub translator_state: ReadSignal<String>,
    pub set_translator_state: WriteSignal<String>,
    pub translator_error: ReadSignal<String>,
    pub set_translator_error: WriteSignal<String>,
    pub is_sniffer_active: ReadSignal<bool>,
    pub set_is_sniffer_active: WriteSignal<bool>,
    pub status_text: ReadSignal<String>,
    pub set_status_text: WriteSignal<String>,
    pub model_ready: ReadSignal<bool>,
    pub set_model_ready: WriteSignal<bool>,
    pub downloading: ReadSignal<bool>,
    pub set_downloading: WriteSignal<bool>,
    pub progress: ReadSignal<u8>,
    pub set_progress: WriteSignal<u8>,
    pub active_tab: ReadSignal<String>,
    pub set_active_tab: WriteSignal<String>,
    pub search_term: ReadSignal<String>,
    pub set_search_term: WriteSignal<String>,
    pub name_cache: ReadSignal<HashMap<String, String>>,
    pub set_name_cache: WriteSignal<HashMap<String, String>>,
    /// Keyed by pid: pids only grow, so key order is arrival order.
    pub chat_log: ReadSignal<BTreeMap<u64, RwSignal<ChatMessage>>>,
    pub set_chat_log: WriteSignal<BTreeMap<u64, RwSignal<ChatMessage>>>,
    pub system_log: ReadSignal<Vec<RwSignal<SystemMessage>>>,
    pub set_system_log: WriteSignal<Vec<RwSignal<SystemMessage>>>,
    pub is_system_at_bottom: ReadSignal<bool>,
    pub set_system_at_bottom: WriteSignal<bool>,
    pub debug_mode: ReadSignal<bool>,
    pub set_debug_mode: WriteSignal<bool>,
    pub log_level: ReadSignal<String>,
    pub set_log_level: WriteSignal<String>,
    pub system_level_filter: ReadSignal<Option<String>>,
    pub set_system_level_filter: WriteSignal<Option<String>>,
    pub system_source_filter: ReadSignal<Option<String>>,
    pub set_system_source_filter: WriteSignal<Option<String>>,
    pub compact_mode: ReadSignal<bool>,
    pub set_compact_mode: WriteSignal<bool>,
    pub is_pinned: ReadSignal<bool>,
    pub set_is_pinned: WriteSignal<bool>,
    pub show_settings: ReadSignal<bool>,
    pub set_show_settings: WriteSignal<bool>,
    pub chat_limit: ReadSignal<usize>,
    pub set_chat_limit: WriteSignal<usize>,
    pub custom_filters: ReadSignal<Vec<String>>,
    pub set_custom_filters: WriteSignal<Vec<String>>,
    pub theme: ReadSignal<String>,
    pub set_theme: WriteSignal<String>,
    pub opacity: ReadSignal<f32>,
    pub set_opacity: WriteSignal<f32>,
    pub tier: ReadSignal<String>,
    pub set_tier: WriteSignal<String>,
    pub restart_required: ReadSignal<bool>,
    pub set_restart_required: WriteSignal<bool>,
    pub dict_update_available: ReadSignal<bool>,
    pub set_dict_update_available: WriteSignal<bool>,
    pub is_at_bottom: ReadSignal<bool>,
    pub set_is_at_bottom: WriteSignal<bool>,
    pub unread_count: ReadSignal<i32>,
    pub set_unread_count: WriteSignal<i32>,
    pub active_menu_id: ReadSignal<Option<u64>>,
    pub set_active_menu_id: WriteSignal<Option<u64>>,
    pub archive_chat: ReadSignal<bool>,
    pub set_archive_chat: WriteSignal<bool>,
    pub hide_original_in_compact: ReadSignal<bool>,
    pub set_hide_original_in_compact: WriteSignal<bool>,
    pub network_interface: ReadSignal<String>,
    pub set_network_interface: WriteSignal<String>,
    pub sniffer_state: ReadSignal<String>,
    pub set_sniffer_state: WriteSignal<String>,
    pub sniffer_error: ReadSignal<String>,
    pub set_sniffer_error: WriteSignal<String>,
    pub click_through: ReadSignal<bool>,
    pub set_click_through: WriteSignal<bool>,
    pub drag_to_scroll: ReadSignal<bool>,
    pub set_drag_to_scroll: WriteSignal<bool>,
    pub alert_keywords: ReadSignal<Vec<String>>,
    pub set_alert_keywords: WriteSignal<Vec<String>>,
    pub alert_volume: ReadSignal<f32>,
    pub set_alert_volume: WriteSignal<f32>,
    pub emphasis_keywords: ReadSignal<Vec<String>>,
    pub set_emphasis_keywords: WriteSignal<Vec<String>>,
    pub use_relative_time: ReadSignal<bool>,
    pub set_use_relative_time: WriteSignal<bool>,
    pub current_time: ReadSignal<u64>,
    pub set_current_time: WriteSignal<u64>,
    pub font_size: ReadSignal<u32>,
    pub set_font_size: WriteSignal<u32>,
    pub hide_blocked_messages: ReadSignal<bool>,
    pub set_hide_blocked_messages: WriteSignal<bool>,
    pub blocked_users: ReadSignal<HashMap<u64, String>>,
    pub set_blocked_users: WriteSignal<HashMap<u64, String>>,
    pub min_sender_level: ReadSignal<u64>,
    pub set_min_sender_level: WriteSignal<u64>,
    pub show_app_update_modal: ReadSignal<bool>,
    pub set_show_app_update_modal: WriteSignal<bool>,
    pub show_model_update_modal: ReadSignal<bool>,
    pub set_show_model_update_modal: WriteSignal<bool>,
    pub pending_update_data: ReadSignal<Option<crate::ui_types::GistMetadata>>,
    pub set_pending_update_data: WriteSignal<Option<crate::ui_types::GistMetadata>>,
    pub app_update_step: ReadSignal<i32>,
    pub set_app_update_step: WriteSignal<i32>,
    pub app_update_progress: ReadSignal<u8>,
    pub set_app_update_progress: WriteSignal<u8>,
    pub model_update_step: ReadSignal<i32>,
    pub set_model_update_step: WriteSignal<i32>,
    pub model_update_progress: ReadSignal<u8>,
    pub set_model_update_progress: WriteSignal<u8>,
    pub show_dictionary: ReadSignal<bool>,
    pub set_show_dictionary: WriteSignal<bool>,
    pub auto_sync_latest_dict: ReadSignal<bool>,
    pub set_auto_sync_latest_dict: WriteSignal<bool>,
    pub unread_counts: ReadSignal<HashMap<String, usize>>,
    pub set_unread_counts: WriteSignal<HashMap<String, usize>>,
    pub tab_switch_modifier: ReadSignal<String>,
    pub set_tab_switch_modifier: WriteSignal<String>,
    pub tab_switch_key: ReadSignal<String>,
    pub set_tab_switch_key: WriteSignal<String>,
    pub show_troubleshooter: ReadSignal<bool>,
    pub set_show_troubleshooter: WriteSignal<bool>,
}

impl AppSignals {
    /// Creates every app-wide signal with its pre-config default. `load_config`
    /// (see `app::hydration`) overwrites most of them at start-up.
    pub fn new() -> Self {
        let (init_done, set_init_done) = signal(false); // Hydrated from config
        let (use_translation, set_use_translation) = signal(false);
        let (compute_mode, set_compute_mode) = signal("cpu".to_string());
        let (wizard_step, set_wizard_step) = signal(0); // 0: Welcome, 1: Options, 2: Download

        let (translator_state, set_translator_state) = signal("Off".to_string());
        let (translator_error, set_translator_error) = signal("".to_string());
        let (is_sniffer_active, set_is_sniffer_active) = signal(false);
        let (status_text, set_status_text) = signal("".to_string());
        let (model_ready, set_model_ready) = signal(false);
        let (downloading, set_downloading) = signal(false);
        let (progress, set_progress) = signal(0u8);

        let (active_tab, set_active_tab) = signal("전체".to_string());
        let (search_term, set_search_term) = signal("".to_string());
        let (name_cache, set_name_cache) =
            signal(std::collections::HashMap::<String, String>::new());
        let (chat_log, set_chat_log) = signal(BTreeMap::<u64, RwSignal<ChatMessage>>::new());
        let (system_log, set_system_log) = signal(Vec::<RwSignal<SystemMessage>>::new());

        let (is_system_at_bottom, set_system_at_bottom) = signal(true);
        let (debug_mode, set_debug_mode) = signal(false);
        let (log_level, set_log_level) = signal("info".to_string());
        let (system_level_filter, set_system_level_filter) = signal(None::<String>);
        let (system_source_filter, set_system_source_filter) = signal(None::<String>);

        let (compact_mode, set_compact_mode) = signal(false);
        let (is_pinned, set_is_pinned) = signal(false);
        let (show_settings, set_show_settings) = signal(false);
        let (chat_limit, set_chat_limit) = signal(1000);
        let (custom_filters, set_custom_filters) = signal(vec![
            "WORLD".to_string(),
            "GUILD".to_string(),
            "PARTY".to_string(),
            "LOCAL".to_string(),
        ]);
        let (theme, set_theme) = signal("dark".to_string());
        let (opacity, set_opacity) = signal(0.85f32);
        let (tier, set_tier) = signal("middle".to_string());
        let (restart_required, set_restart_required) = signal(false);
        let (dict_update_available, set_dict_update_available) = signal(false);
        let (is_at_bottom, set_is_at_bottom) = signal(true);
        let (unread_count, set_unread_count) = signal(0);
        let (active_menu_id, set_active_menu_id) = signal(None::<u64>);
        let (archive_chat, set_archive_chat) = signal(false);
        let (hide_original_in_compact, set_hide_original_in_compact) = signal(false);
        let (network_interface, set_network_interface) = signal("".to_string());
        let (click_through, set_click_through) = signal(false);
        let (drag_to_scroll, set_drag_to_scroll) = signal(false);

        let (sniffer_state, set_sniffer_state) = signal("Off".to_string());
        let (sniffer_error, set_sniffer_error) = signal("".to_string());

        let (alert_keywords, set_alert_keywords) = signal(Vec::<String>::new());
        let (alert_volume, set_alert_volume) = signal(0.5f32);
        let (emphasis_keywords, set_emphasis_keywords) = signal(Vec::<String>::new());
        let (use_relative_time, set_use_relative_time) = signal(false);
        let (current_time, set_current_time) =
            signal(chrono::Local::now().timestamp_millis() as u64);
        let (font_size, set_font_size) = signal(14u32);
        let (hide_blocked_messages, set_hide_blocked_messages) = signal(false);
        let (blocked_users, set_blocked_users) =
            signal::<std::collections::HashMap<u64, String>>(HashMap::new());
        let (min_sender_level, set_min_sender_level) = signal(1);

        let (show_app_update_modal, set_show_app_update_modal) = signal(false);
        let (show_model_update_modal, set_show_model_update_modal) = signal(false);
        let (pending_update_data, set_pending_update_data) =
            signal(None::<crate::ui_types::GistMetadata>);

        // --- APP UPDATE TRACKING STATES ---
        let (app_update_step, set_app_update_step) = signal(0); // 0: Info, 1: Downloading, 2: Ready
        let (app_update_progress, set_app_update_progress) = signal(0u8);

        // --- MODEL UPDATE TRACKING STATES ---
        let (model_update_step, set_model_update_step) = signal(0); // 0: Info, 1: Downloading, 2: Ready
        let (model_update_progress, set_model_update_progress) = signal(0u8);

        let (auto_sync_latest_dict, set_auto_sync_latest_dict) = signal(true);
        let (show_dictionary, set_show_dictionary) = signal(false);
        let (unread_counts, set_unread_counts) =
            signal::<std::collections::HashMap<String, usize>>(HashMap::new());

        let (tab_switch_modifier, set_tab_switch_modifier) = signal("Ctrl".to_string());
        let (tab_switch_key, set_tab_switch_key) = signal("Tab".to_string());

        let (show_troubleshooter, set_show_troubleshooter) = signal(false);

        AppSignals {
            init_done,
            set_init_done,
            use_translation,
            set_use_translation,
            compute_mode,
            set_compute_mode,
            wizard_step,
            set_wizard_step,
            translator_state,
            set_translator_state,
            translator_error,
            set_translator_error,
            is_sniffer_active,
            set_is_sniffer_active,
            status_text,
            set_status_text,
            model_ready,
            set_model_ready,
            downloading,
            set_downloading,
            progress,
            set_progress,
            active_tab,
            set_active_tab,
            search_term,
            set_search_term,
            name_cache,
            set_name_cache,
            chat_log,
            set_chat_log,
            system_log,
            set_system_log,
            is_system_at_bottom,
            set_system_at_bottom,
            debug_mode,
            set_debug_mode,
            log_level,
            set_log_level,
            system_level_filter,
            set_system_level_filter,
            system_source_filter,
            set_system_source_filter,
            compact_mode,
            set_compact_mode,
            is_pinned,
            set_is_pinned,
            show_settings,
            set_show_settings,
            chat_limit,
            set_chat_limit,
            custom_filters,
            set_custom_filters,
            theme,
            set_theme,
            opacity,
            set_opacity,
            tier,
            set_tier,
            restart_required,
            set_restart_required,
            dict_update_available,
            set_dict_update_available,
            is_at_bottom,
            set_is_at_bottom,
            unread_count,
            set_unread_count,
            active_menu_id,
            set_active_menu_id,
            archive_chat,
            set_archive_chat,
            hide_original_in_compact,
            set_hide_original_in_compact,
            network_interface,
            set_network_interface,
            click_through,
            set_click_through,
            drag_to_scroll,
            set_drag_to_scroll,
            sniffer_state,
            set_sniffer_state,
            sniffer_error,
            set_sniffer_error,
            alert_keywords,
            set_alert_keywords,
            alert_volume,
            set_alert_volume,
            emphasis_keywords,
            set_emphasis_keywords,
            use_relative_time,
            set_use_relative_time,
            current_time,
            set_current_time,
            font_size,
            set_font_size,
            hide_blocked_messages,
            set_hide_blocked_messages,
            blocked_users,
            set_blocked_users,
            min_sender_level,
            set_min_sender_level,
            app_update_step,
            set_app_update_step,
            app_update_progress,
            set_app_update_progress,
            model_update_step,
            set_model_update_step,
            model_update_progress,
            set_model_update_progress,
            show_app_update_modal,
            set_show_app_update_modal,
            show_model_update_modal,
            set_show_model_update_modal,
            pending_update_data,
            set_pending_update_data,
            auto_sync_latest_dict,
            set_auto_sync_latest_dict,
            show_dictionary,
            set_show_dictionary,
            unread_counts,
            set_unread_counts,
            tab_switch_modifier,
            set_tab_switch_modifier,
            tab_switch_key,
            set_tab_switch_key,
            show_troubleshooter,
            set_show_troubleshooter,
        }
    }
}

#[derive(Copy, Clone)]
pub struct AppActions {
    pub save_config: Action<(), ()>,
    pub clear_history: Action<(), ()>,
}
