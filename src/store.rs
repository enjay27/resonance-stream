use crate::chat_view::ChatStore;
use crate::config_signals::ConfigSignals;
use crate::status_signals::{ServiceSignals, SetupSignals, UpdateSignals};
use crate::ui_types::{ChatMessage, SystemLogLevel, SystemMessage};
use leptos::prelude::{signal, Action, ArcRwSignal, ReadSignal, WriteSignal};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug)]
pub struct AppSignals {
    /// The signals that mirror `config.json`; see `config_signals.rs`.
    pub config: ConfigSignals,
    pub service: ServiceSignals,
    pub setup: SetupSignals,
    pub updates: UpdateSignals,
    // Add other global controls here
    pub search_term: ReadSignal<String>,
    pub set_search_term: WriteSignal<String>,
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
    pub is_at_bottom: ReadSignal<bool>,
    pub set_is_at_bottom: WriteSignal<bool>,
    pub unread_count: ReadSignal<i32>,
    pub set_unread_count: WriteSignal<i32>,
    pub active_menu_id: ReadSignal<Option<u64>>,
    pub set_active_menu_id: WriteSignal<Option<u64>>,
    pub click_through: ReadSignal<bool>,
    pub set_click_through: WriteSignal<bool>,
    pub current_time: ReadSignal<u64>,
    pub set_current_time: WriteSignal<u64>,
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
        let (search_term, set_search_term) = signal("".to_string());
        let (chat, set_chat) = signal(ChatStore::<ArcRwSignal<ChatMessage>>::default());
        let (system_log, set_system_log) = signal(Vec::<ArcRwSignal<SystemMessage>>::new());

        let (is_system_at_bottom, set_system_at_bottom) = signal(true);
        let (system_level_filter, set_system_level_filter) = signal(None::<SystemLogLevel>);
        let (system_source_filter, set_system_source_filter) = signal(None::<String>);

        let (show_settings, set_show_settings) = signal(false);
        let (is_at_bottom, set_is_at_bottom) = signal(true);
        let (unread_count, set_unread_count) = signal(0);
        let (active_menu_id, set_active_menu_id) = signal(None::<u64>);
        let (click_through, set_click_through) = signal(false);

        let (current_time, set_current_time) =
            signal(chrono::Local::now().timestamp_millis() as u64);

        // --- APP UPDATE TRACKING STATES ---

        // --- MODEL UPDATE TRACKING STATES ---

        let (show_dictionary, set_show_dictionary) = signal(false);
        let (unread_counts, set_unread_counts) =
            signal::<std::collections::HashMap<String, usize>>(HashMap::new());

        let (show_troubleshooter, set_show_troubleshooter) = signal(false);
        let (show_favorites, set_show_favorites) = signal(false);

        AppSignals {
            config: ConfigSignals::new(),
            service: ServiceSignals::new(),
            setup: SetupSignals::new(),
            updates: UpdateSignals::new(),
            search_term,
            set_search_term,
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
            is_at_bottom,
            set_is_at_bottom,
            unread_count,
            set_unread_count,
            active_menu_id,
            set_active_menu_id,
            click_through,
            set_click_through,
            current_time,
            set_current_time,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_types::{SnifferState, TranslatorState};
    use leptos::prelude::GetUntracked;

    #[test]
    fn the_backend_status_signals_start_quiet() {
        // What the ui shows before the backend has said anything.
        let s = AppSignals::new();
        assert_eq!(
            s.service.translator_state.get_untracked(),
            TranslatorState::Off
        );
        assert_eq!(s.service.sniffer_state.get_untracked(), SnifferState::Off);
        assert!(s.service.translator_error.get_untracked().is_empty());
        assert!(s.service.sniffer_error.get_untracked().is_empty());
        assert!(!s.service.is_sniffer_active.get_untracked());
        assert!(!s.service.model_ready.get_untracked());
        assert!(!s.service.restart_required.get_untracked());
        assert!(!s.service.dict_update_available.get_untracked());
    }

    #[test]
    fn the_setup_and_update_signals_start_idle() {
        let s = AppSignals::new();
        assert_eq!(s.setup.wizard_step.get_untracked(), 0);
        assert!(s.setup.status_text.get_untracked().is_empty());
        assert!(!s.setup.downloading.get_untracked());
        assert_eq!(s.setup.progress.get_untracked(), 0);
        assert!(!s.updates.show_app_update_modal.get_untracked());
        assert!(!s.updates.show_model_update_modal.get_untracked());
        assert!(s.updates.pending_update_data.get_untracked().is_none());
        assert_eq!(s.updates.app_update_step.get_untracked(), 0);
        assert_eq!(s.updates.model_update_step.get_untracked(), 0);
        assert_eq!(s.updates.app_update_progress.get_untracked(), 0);
        assert_eq!(s.updates.model_update_progress.get_untracked(), 0);
    }
}
