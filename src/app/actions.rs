//! The two app-wide actions every component reaches through `AppActions`.

use crate::chat_view::ChatStore;
use crate::hooks::use_config::save_app_config;
use crate::store::{AppActions, AppSignals};
use crate::ui_types::AppConfig;
use futures::FutureExt;
use leptos::prelude::*;
use std::collections::HashMap;

pub fn create_actions(signals: AppSignals) -> AppActions {
    let AppSignals {
        init_done,
        use_translation,
        compute_mode,
        active_tab,
        set_chat,
        set_system_log,
        debug_mode,
        log_level,
        compact_mode,
        is_pinned,
        tab_limits,
        archive_ignored_channels,
        message_spacing,
        custom_filters,
        theme,
        opacity,
        tier,
        translation_catch_up_limit,
        set_unread_count,
        archive_chat,
        hide_original_in_compact,
        network_interface,
        drag_to_scroll,
        alert_keywords,
        alert_volume,
        emphasis_keywords,
        use_relative_time,
        font_size,
        hide_blocked_messages,
        blocked_users,
        min_sender_level,
        auto_sync_latest_dict,
        set_unread_counts,
        tab_switch_modifier,
        tab_switch_key,
        favorite_messages,
        chat_log_retention_days,
        ..
    } = signals;

    // --- CONFIG ACTIONS ---
    let save_config = Action::new_local(move |_: &()| {
        let config = AppConfig {
            init_done: init_done.get_untracked(),
            use_translation: use_translation.get_untracked(),
            compute_mode: compute_mode.get_untracked(),
            compact_mode: compact_mode.get_untracked(),
            always_on_top: is_pinned.get_untracked(),
            active_tab: active_tab.get_untracked(),
            custom_tab_filters: custom_filters.get_untracked(),
            theme: theme.get_untracked(),
            overlay_opacity: opacity.get_untracked(),
            debug_mode: debug_mode.get_untracked(),
            log_level: log_level.get_untracked(),
            tier: tier.get_untracked(),
            translation_catch_up_limit: translation_catch_up_limit.get_untracked(),
            archive_chat: archive_chat.get_untracked(),
            hide_original_in_compact: hide_original_in_compact.get_untracked(),
            network_interface: network_interface.get_untracked(),
            drag_to_scroll: drag_to_scroll.get_untracked(),
            alert_keywords: alert_keywords.get_untracked(),
            alert_volume: alert_volume.get_untracked(),
            emphasis_keywords: emphasis_keywords.get_untracked(),
            use_relative_time: use_relative_time.get_untracked(),
            font_size: font_size.get_untracked(),
            hide_blocked_messages: hide_blocked_messages.get_untracked(),
            blocked_users: blocked_users.get_untracked(),
            min_sender_level: min_sender_level.get_untracked(),
            auto_sync_latest_dict: auto_sync_latest_dict.get_untracked(),
            tab_switch_modifier: tab_switch_modifier.get_untracked(),
            tab_limits: tab_limits.get_untracked(),
            archive_ignored_channels: archive_ignored_channels.get_untracked(),
            message_spacing: message_spacing.get_untracked(),
            tab_switch_key: tab_switch_key.get_untracked(),
            favorite_messages: favorite_messages.get_untracked(),
            chat_log_retention_days: chat_log_retention_days.get_untracked(),
        };

        async move {
            save_app_config(config).await;
        }
    });

    // Action: Clear Chat
    let clear_history = Action::new_local(move |_: &()| {
        let confirmed = window()
            .confirm_with_message("Clear all chat history?")
            .unwrap_or(false);

        async move {
            if confirmed {
                crate::hooks::use_events::clear_backend_history().await;
                set_chat.set(ChatStore::default());
                set_system_log.set(Vec::new());

                // NEW: Clear the badges when history is wiped
                set_unread_count.set(0);
                set_unread_counts.set(HashMap::new());
            }
        }
        .boxed_local()
    });

    AppActions {
        save_config,
        clear_history,
    }
}
