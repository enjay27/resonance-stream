//! The chat list and the view around it: what is shown, where it is scrolled,
//! which dialogs are open. (Settings are `config_signals.rs`, what the backend
//! reports is `status_signals.rs`.)

use crate::chat_view::ChatStore;
use crate::settings_nav::SettingsCategory;
use crate::ui_types::{ChatMessage, SystemLogLevel, SystemMessage};
use leptos::prelude::{signal, ArcRwSignal, ReadSignal, WriteSignal};
use std::collections::HashMap;

/// Which of a chat row's two menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// Opened by the sender's name: copy name, filter, block.
    Sender,
    /// Opened by the message text: copy, favorite, add to the dictionary.
    Message,
}

/// An open chat row menu: the message (by pid) and which menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowMenu {
    pub pid: u64,
    pub kind: MenuKind,
}

impl RowMenu {
    /// What a click on `target` leaves open: its menu, or nothing when that
    /// menu was the one open (a second click closes it).
    pub fn toggled(open: Option<RowMenu>, target: RowMenu) -> Option<RowMenu> {
        if open == Some(target) {
            None
        } else {
            Some(target)
        }
    }
}

/// A term on its way into the dictionary: the dialog's starting key and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictDraft {
    pub key: String,
    pub value: String,
}

/// The chat list and system log and what the user is doing with them: search, scroll position, unread counts.
#[derive(Copy, Clone, Debug)]
pub struct ChatSignals {
    /// Chat messages by pid, plus each tab's list (per-tab limits).
    /// `ArcRwSignal`: rows are created in event callbacks, where no reactive
    /// owner exists, so an arena `RwSignal` would never be freed; an Arc one
    /// is freed with the last clone (evicted here, and its row unmounted).
    pub chat: ReadSignal<ChatStore<ArcRwSignal<ChatMessage>>>,
    pub set_chat: WriteSignal<ChatStore<ArcRwSignal<ChatMessage>>>,
    pub system_log: ReadSignal<Vec<ArcRwSignal<SystemMessage>>>,
    pub set_system_log: WriteSignal<Vec<ArcRwSignal<SystemMessage>>>,
    pub is_system_at_bottom: ReadSignal<bool>,
    pub set_is_system_at_bottom: WriteSignal<bool>,
    pub system_level_filter: ReadSignal<Option<SystemLogLevel>>,
    pub set_system_level_filter: WriteSignal<Option<SystemLogLevel>>,
    pub system_source_filter: ReadSignal<Option<String>>,
    pub set_system_source_filter: WriteSignal<Option<String>>,
    pub search_term: ReadSignal<String>,
    pub set_search_term: WriteSignal<String>,
    pub is_at_bottom: ReadSignal<bool>,
    pub set_is_at_bottom: WriteSignal<bool>,
    pub unread_count: ReadSignal<i32>,
    pub set_unread_count: WriteSignal<i32>,
    pub unread_counts: ReadSignal<HashMap<String, usize>>,
    pub set_unread_counts: WriteSignal<HashMap<String, usize>>,
    pub current_time: ReadSignal<u64>,
    pub set_current_time: WriteSignal<u64>,
}

impl ChatSignals {
    pub fn new() -> Self {
        let (chat, set_chat) = signal::<ChatStore<ArcRwSignal<ChatMessage>>>(ChatStore::default());
        let (system_log, set_system_log) = signal::<Vec<ArcRwSignal<SystemMessage>>>(Vec::new());
        let (is_system_at_bottom, set_is_system_at_bottom) = signal::<bool>(true);
        let (system_level_filter, set_system_level_filter) = signal::<Option<SystemLogLevel>>(None);
        let (system_source_filter, set_system_source_filter) = signal::<Option<String>>(None);
        let (search_term, set_search_term) = signal::<String>(String::new());
        let (is_at_bottom, set_is_at_bottom) = signal::<bool>(true);
        let (unread_count, set_unread_count) = signal::<i32>(0);
        let (unread_counts, set_unread_counts) = signal::<HashMap<String, usize>>(HashMap::new());
        let (current_time, set_current_time) =
            signal::<u64>(chrono::Local::now().timestamp_millis() as u64);
        ChatSignals {
            chat,
            set_chat,
            system_log,
            set_system_log,
            is_system_at_bottom,
            set_is_system_at_bottom,
            system_level_filter,
            set_system_level_filter,
            system_source_filter,
            set_system_source_filter,
            search_term,
            set_search_term,
            is_at_bottom,
            set_is_at_bottom,
            unread_count,
            set_unread_count,
            unread_counts,
            set_unread_counts,
            current_time,
            set_current_time,
        }
    }
}

/// Which dialogs and menus are open, and the window's click-through mode.
#[derive(Copy, Clone, Debug)]
pub struct UiSignals {
    pub show_settings: ReadSignal<bool>,
    pub set_show_settings: WriteSignal<bool>,
    /// The settings pane shown; kept while the settings view is closed.
    pub settings_category: ReadSignal<SettingsCategory>,
    pub set_settings_category: WriteSignal<SettingsCategory>,
    pub show_dictionary: ReadSignal<bool>,
    pub set_show_dictionary: WriteSignal<bool>,
    pub show_troubleshooter: ReadSignal<bool>,
    pub set_show_troubleshooter: WriteSignal<bool>,
    pub show_favorites: ReadSignal<bool>,
    pub set_show_favorites: WriteSignal<bool>,
    /// The chat row menu that is open, if any (one at a time).
    pub active_menu: ReadSignal<Option<RowMenu>>,
    pub set_active_menu: WriteSignal<Option<RowMenu>>,
    /// The term being added to the dictionary from a chat message; the dialog is open while it is set.
    pub dict_draft: ReadSignal<Option<DictDraft>>,
    pub set_dict_draft: WriteSignal<Option<DictDraft>>,
    pub click_through: ReadSignal<bool>,
    pub set_click_through: WriteSignal<bool>,
}

impl UiSignals {
    pub fn new() -> Self {
        let (show_settings, set_show_settings) = signal::<bool>(false);
        let (settings_category, set_settings_category) =
            signal::<SettingsCategory>(SettingsCategory::default());
        let (show_dictionary, set_show_dictionary) = signal::<bool>(false);
        let (show_troubleshooter, set_show_troubleshooter) = signal::<bool>(false);
        let (show_favorites, set_show_favorites) = signal::<bool>(false);
        let (active_menu, set_active_menu) = signal::<Option<RowMenu>>(None);
        let (dict_draft, set_dict_draft) = signal::<Option<DictDraft>>(None);
        let (click_through, set_click_through) = signal::<bool>(false);
        UiSignals {
            show_settings,
            set_show_settings,
            settings_category,
            set_settings_category,
            show_dictionary,
            set_show_dictionary,
            show_troubleshooter,
            set_show_troubleshooter,
            show_favorites,
            set_show_favorites,
            active_menu,
            set_active_menu,
            dict_draft,
            set_dict_draft,
            click_through,
            set_click_through,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: RowMenu = RowMenu {
        pid: 7,
        kind: MenuKind::Sender,
    };
    const TEXT: RowMenu = RowMenu {
        pid: 7,
        kind: MenuKind::Message,
    };

    #[test]
    fn a_click_opens_its_menu_and_a_second_click_closes_it() {
        assert_eq!(RowMenu::toggled(None, NAME), Some(NAME));
        assert_eq!(RowMenu::toggled(Some(NAME), NAME), None);
    }

    #[test]
    fn the_name_and_the_text_of_a_row_have_separate_menus() {
        assert_eq!(RowMenu::toggled(Some(NAME), TEXT), Some(TEXT));
        assert_eq!(RowMenu::toggled(Some(TEXT), NAME), Some(NAME));
    }

    #[test]
    fn another_rows_menu_is_replaced() {
        let other = RowMenu {
            pid: 8,
            kind: MenuKind::Message,
        };
        assert_eq!(RowMenu::toggled(Some(TEXT), other), Some(other));
    }
}
