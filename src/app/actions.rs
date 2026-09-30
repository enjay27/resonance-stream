//! The two app-wide actions every component reaches through `AppActions`.

use crate::chat_view::ChatStore;
use crate::hooks::use_config::save_app_config;
use crate::store::{AppActions, AppSignals};
use crate::view_signals::ChatSignals;
use futures::FutureExt;
use leptos::prelude::*;
use std::collections::HashMap;

pub fn create_actions(signals: AppSignals) -> AppActions {
    let ChatSignals {
        set_chat,
        set_system_log,
        set_unread_count,
        set_unread_counts,
        ..
    } = signals.chat;

    // --- CONFIG ACTIONS ---
    let save_config = Action::new_local(move |_: &()| {
        let config = signals.config.to_config();

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
