use crate::chat_view::ChatStore;
use crate::config_signals::ConfigSignals;
use crate::ui_types::{ChatMessage, SnifferState, SystemLogLevel, SystemMessage, TranslatorState};
use leptos::prelude::{signal, Action, ArcRwSignal, ReadSignal, WriteSignal};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug)]
pub struct AppSignals {
    /// The signals that mirror `config.json`; see `config_signals.rs`.
    pub config: ConfigSignals,
    // Add other global controls here
    pub wizard_step: ReadSignal<i32>,
    pub set_wizard_step: WriteSignal<i32>,
    pub translator_state: ReadSignal<TranslatorState>,
    pub set_translator_state: WriteSignal<TranslatorState>,
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
    pub search_term: ReadSignal<String>,
    pub set_search_term: WriteSignal<String>,
    pub name_cache: ReadSignal<HashMap<String, String>>,
    pub set_name_cache: WriteSignal<HashMap<String, String>>,
    /// Chat messages by pid, plus each tab's list (per-tab limits).
    /// `ArcRwSignal`: rows are created in event callbacks, where no reactive
    /// owner exists, so an arena `RwSignal` would never be freed; an Arc one
    /// is freed with the last clone (evicted here, and its row unmounted).
    pub chat: ReadSignal<ChatStore<ArcRwSignal<ChatMessage>>>,
    pub set_chat: WriteSignal<ChatStore<ArcRwSignal<ChatMessage>>>,
    pub system_log: ReadSignal<Vec<ArcRwSignal<SystemMessage>>>,
    pub set_system_log: WriteSignal<Vec<ArcRwSignal<SystemMessage>>>,
    pub is_system_at_bottom: ReadSignal<bool>,
    pub set_system_at_bottom: WriteSignal<bool>,
    pub system_level_filter: ReadSignal<Option<SystemLogLevel>>,
    pub set_system_level_filter: WriteSignal<Option<SystemLogLevel>>,
    pub system_source_filter: ReadSignal<Option<String>>,
    pub set_system_source_filter: WriteSignal<Option<String>>,
    pub show_settings: ReadSignal<bool>,
    pub set_show_settings: WriteSignal<bool>,
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
    pub sniffer_state: ReadSignal<SnifferState>,
    pub set_sniffer_state: WriteSignal<SnifferState>,
    pub sniffer_error: ReadSignal<String>,
    pub set_sniffer_error: WriteSignal<String>,
    pub click_through: ReadSignal<bool>,
    pub set_click_through: WriteSignal<bool>,
    pub current_time: ReadSignal<u64>,
    pub set_current_time: WriteSignal<u64>,
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
    pub unread_counts: ReadSignal<HashMap<String, usize>>,
    pub set_unread_counts: WriteSignal<HashMap<String, usize>>,
    pub show_troubleshooter: ReadSignal<bool>,
    pub set_show_troubleshooter: WriteSignal<bool>,
    pub show_favorites: ReadSignal<bool>,
    pub set_show_favorites: WriteSignal<bool>,
}

impl AppSignals {
    /// Creates every app-wide signal with its pre-config default. `load_config`
    /// (see `app::hydration`) overwrites most of them at start-up.
    pub fn new() -> Self {
        let (wizard_step, set_wizard_step) = signal(0); // 0: Welcome, 1: Options, 2: Download

        let (translator_state, set_translator_state) = signal(TranslatorState::Off);
        let (translator_error, set_translator_error) = signal("".to_string());
        let (is_sniffer_active, set_is_sniffer_active) = signal(false);
        let (status_text, set_status_text) = signal("".to_string());
        let (model_ready, set_model_ready) = signal(false);
        let (downloading, set_downloading) = signal(false);
        let (progress, set_progress) = signal(0u8);

        let (search_term, set_search_term) = signal("".to_string());
        let (name_cache, set_name_cache) =
            signal(std::collections::HashMap::<String, String>::new());
        let (chat, set_chat) = signal(ChatStore::<ArcRwSignal<ChatMessage>>::default());
        let (system_log, set_system_log) = signal(Vec::<ArcRwSignal<SystemMessage>>::new());

        let (is_system_at_bottom, set_system_at_bottom) = signal(true);
        let (system_level_filter, set_system_level_filter) = signal(None::<SystemLogLevel>);
        let (system_source_filter, set_system_source_filter) = signal(None::<String>);

        let (show_settings, set_show_settings) = signal(false);
        let (restart_required, set_restart_required) = signal(false);
        let (dict_update_available, set_dict_update_available) = signal(false);
        let (is_at_bottom, set_is_at_bottom) = signal(true);
        let (unread_count, set_unread_count) = signal(0);
        let (active_menu_id, set_active_menu_id) = signal(None::<u64>);
        let (click_through, set_click_through) = signal(false);

        let (sniffer_state, set_sniffer_state) = signal(SnifferState::Off);
        let (sniffer_error, set_sniffer_error) = signal("".to_string());

        let (current_time, set_current_time) =
            signal(chrono::Local::now().timestamp_millis() as u64);

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

        let (show_dictionary, set_show_dictionary) = signal(false);
        let (unread_counts, set_unread_counts) =
            signal::<std::collections::HashMap<String, usize>>(HashMap::new());

        let (show_troubleshooter, set_show_troubleshooter) = signal(false);
        let (show_favorites, set_show_favorites) = signal(false);

        AppSignals {
            config: ConfigSignals::new(),
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
            search_term,
            set_search_term,
            name_cache,
            set_name_cache,
            chat,
            set_chat,
            system_log,
            set_system_log,
            is_system_at_bottom,
            set_system_at_bottom,
            system_level_filter,
            set_system_level_filter,
            system_source_filter,
            set_system_source_filter,
            show_settings,
            set_show_settings,
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
            click_through,
            set_click_through,
            sniffer_state,
            set_sniffer_state,
            sniffer_error,
            set_sniffer_error,
            current_time,
            set_current_time,
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
            show_dictionary,
            set_show_dictionary,
            unread_counts,
            set_unread_counts,
            show_troubleshooter,
            set_show_troubleshooter,
            show_favorites,
            set_show_favorites,
        }
    }
}

#[derive(Copy, Clone)]
pub struct AppActions {
    pub save_config: Action<(), ()>,
    pub clear_history: Action<(), ()>,
}
