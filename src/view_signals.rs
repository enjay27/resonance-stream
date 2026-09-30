//! The chat list and the view around it: what is shown, where it is scrolled,
//! which dialogs are open. (Settings are `config_signals.rs`, what the backend
//! reports is `status_signals.rs`.)

use crate::chat_view::ChatStore;
use crate::ui_types::{ChatMessage, SystemLogLevel, SystemMessage};
use leptos::prelude::{signal, ArcRwSignal, ReadSignal, WriteSignal};
use std::collections::HashMap;

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
    pub show_dictionary: ReadSignal<bool>,
    pub set_show_dictionary: WriteSignal<bool>,
    pub show_troubleshooter: ReadSignal<bool>,
    pub set_show_troubleshooter: WriteSignal<bool>,
    pub show_favorites: ReadSignal<bool>,
    pub set_show_favorites: WriteSignal<bool>,
    pub active_menu_id: ReadSignal<Option<u64>>,
    pub set_active_menu_id: WriteSignal<Option<u64>>,
    pub click_through: ReadSignal<bool>,
    pub set_click_through: WriteSignal<bool>,
}

impl UiSignals {
    pub fn new() -> Self {
        let (show_settings, set_show_settings) = signal::<bool>(false);
        let (show_dictionary, set_show_dictionary) = signal::<bool>(false);
        let (show_troubleshooter, set_show_troubleshooter) = signal::<bool>(false);
        let (show_favorites, set_show_favorites) = signal::<bool>(false);
        let (active_menu_id, set_active_menu_id) = signal::<Option<u64>>(None);
        let (click_through, set_click_through) = signal::<bool>(false);
        UiSignals {
            show_settings,
            set_show_settings,
            show_dictionary,
            set_show_dictionary,
            show_troubleshooter,
            set_show_troubleshooter,
            show_favorites,
            set_show_favorites,
            active_menu_id,
            set_active_menu_id,
            click_through,
            set_click_through,
        }
    }
}
