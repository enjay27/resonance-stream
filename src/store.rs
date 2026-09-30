use crate::config_signals::ConfigSignals;
use crate::status_signals::{ServiceSignals, SetupSignals, UpdateSignals};
use crate::view_signals::{ChatSignals, UiSignals};
use leptos::prelude::Action;

/// Every app-wide signal, in groups by what it describes; components reach a
/// signal as `signals.<group>.<name>` (`signals.config.theme`,
/// `signals.service.model_ready`, `signals.chat.search_term`).
#[derive(Copy, Clone, Debug)]
pub struct AppSignals {
    /// Mirrors `config.json`, one signal per `AppConfig` field (`config_signals.rs`).
    pub config: ConfigSignals,
    /// What the translator and the sniffer report (`status_signals.rs`).
    pub service: ServiceSignals,
    /// The first-run wizard and its download progress (`status_signals.rs`).
    pub setup: SetupSignals,
    /// The app and model update dialogs (`status_signals.rs`).
    pub updates: UpdateSignals,
    /// The chat list, the system log, scroll and unread state (`view_signals.rs`).
    pub chat: ChatSignals,
    /// Open dialogs and menus, click-through (`view_signals.rs`).
    pub ui: UiSignals,
}

impl AppSignals {
    /// Creates every app-wide signal with its pre-config default. `load_config`
    /// (see `app::hydration`) overwrites most of them at start-up.
    pub fn new() -> Self {
        AppSignals {
            config: ConfigSignals::new(),
            service: ServiceSignals::new(),
            setup: SetupSignals::new(),
            updates: UpdateSignals::new(),
            chat: ChatSignals::new(),
            ui: UiSignals::new(),
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

    #[test]
    fn the_chat_signals_start_empty_and_at_the_bottom() {
        let s = AppSignals::new();
        assert!(s.chat.search_term.get_untracked().is_empty());
        assert_eq!(s.chat.chat.get_untracked().len(), 0);
        assert!(s.chat.system_log.get_untracked().is_empty());
        assert!(s.chat.is_system_at_bottom.get_untracked());
        assert!(s.chat.is_at_bottom.get_untracked());
        assert!(s.chat.system_level_filter.get_untracked().is_none());
        assert!(s.chat.system_source_filter.get_untracked().is_none());
        assert_eq!(s.chat.unread_count.get_untracked(), 0);
        assert!(s.chat.unread_counts.get_untracked().is_empty());
        assert!(s.chat.current_time.get_untracked() > 1_700_000_000_000); // a real clock, in ms
    }

    #[test]
    fn the_ui_signals_start_closed() {
        let s = AppSignals::new();
        assert!(!s.ui.show_settings.get_untracked());
        assert!(!s.ui.show_dictionary.get_untracked());
        assert!(!s.ui.show_troubleshooter.get_untracked());
        assert!(!s.ui.show_favorites.get_untracked());
        assert!(s.ui.active_menu_id.get_untracked().is_none());
        assert!(!s.ui.click_through.get_untracked());
    }
}
