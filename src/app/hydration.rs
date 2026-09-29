//! Start-up: load config into the signals and, for a returning user, restore
//! history, start the sniffer and translator, and check for updates.

use crate::hooks::use_events::setup_event_listeners;
use crate::store::AppSignals;
use crate::tauri_bridge::invoke;
use crate::ui_types::{AppConfig, ChatMessage, FolderStatus, SystemMessage};
use crate::utils::add_system_log;
use leptos::leptos_dom::log;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

pub async fn hydrate_from_backend(signals: AppSignals) {
    let AppSignals {
        set_init_done,
        set_use_translation,
        set_compute_mode,
        set_is_sniffer_active,
        set_status_text,
        set_model_ready,
        set_active_tab,
        set_chat_log,
        set_system_log,
        set_debug_mode,
        set_log_level,
        set_compact_mode,
        set_is_pinned,
        set_chat_limit,
        set_custom_filters,
        set_theme,
        set_opacity,
        set_tier,
        set_dict_update_available,
        set_archive_chat,
        set_hide_original_in_compact,
        set_network_interface,
        set_drag_to_scroll,
        set_alert_keywords,
        set_alert_volume,
        set_emphasis_keywords,
        set_use_relative_time,
        set_font_size,
        set_hide_blocked_messages,
        set_blocked_users,
        set_min_sender_level,
        set_show_app_update_modal,
        set_show_model_update_modal,
        set_pending_update_data,
        set_auto_sync_latest_dict,
        set_tab_switch_modifier,
        set_tab_switch_key,
        ..
    } = signals;

    log!("App component hydration started...");
    // Load User Config
    match invoke("load_config", JsValue::NULL).await {
        Ok(res) => {
            if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(res) {
                log!("Loaded Config: {:?}", config);
                set_init_done.set(config.init_done);
                set_use_translation.set(config.use_translation);
                set_compute_mode.set(config.compute_mode);
                set_compact_mode.set(config.compact_mode);
                set_active_tab.set(config.active_tab);
                set_is_pinned.set(config.always_on_top);
                set_chat_limit.set(config.chat_limit);
                set_custom_filters.set(config.custom_tab_filters);
                set_theme.set(config.theme);
                set_opacity.set(config.overlay_opacity);
                set_debug_mode.set(config.debug_mode);
                set_log_level.set(config.log_level);
                set_tier.set(config.tier);
                set_archive_chat.set(config.archive_chat);
                set_hide_original_in_compact.set(config.hide_original_in_compact);
                set_network_interface.set(config.network_interface);
                set_drag_to_scroll.set(config.drag_to_scroll);
                set_alert_keywords.set(config.alert_keywords);
                set_alert_volume.set(config.alert_volume);
                set_emphasis_keywords.set(config.emphasis_keywords);
                set_use_relative_time.set(config.use_relative_time);
                let loaded_size = if config.font_size > 8 {
                    config.font_size
                } else {
                    14
                };
                set_font_size.set(loaded_size);
                set_hide_blocked_messages.set(config.hide_blocked_messages);
                set_blocked_users.set(config.blocked_users);
                set_min_sender_level.set(config.min_sender_level);
                set_auto_sync_latest_dict.set(config.auto_sync_latest_dict);
                set_tab_switch_modifier.set(if config.tab_switch_modifier.is_empty() {
                    "Ctrl".to_string()
                } else {
                    config.tab_switch_modifier
                });
                set_tab_switch_key.set(if config.tab_switch_key.is_empty() {
                    "Tab".to_string()
                } else {
                    config.tab_switch_key
                });

                // 2. If the user hasn't finished the wizard, stop here
                if config.init_done {
                    log!("Existing user detected. Auto-starting services.");
                    add_system_log("info", "Sniffer", "Auto-starting services...");
                    setup_event_listeners(signals).await;

                    // Hydrate GAME History
                    if let Ok(res) = invoke("get_chat_history", JsValue::NULL).await {
                        if let Ok(vec) = serde_wasm_bindgen::from_value::<Vec<ChatMessage>>(res) {
                            // Stickers/emotes arrive already normalized by the backend.
                            set_chat_log
                                .set(vec.into_iter().map(|p| (p.pid, RwSignal::new(p))).collect());
                        }
                    }

                    // Hydrate SYSTEM History
                    if let Ok(res) = invoke("get_system_history", JsValue::NULL).await {
                        if let Ok(vec) = serde_wasm_bindgen::from_value::<Vec<SystemMessage>>(res) {
                            set_system_log.set(vec.into_iter().map(|p| RwSignal::new(p)).collect());
                        }
                    }
                    set_is_sniffer_active.set(true);
                    let _ = invoke("start_sniffer_command", JsValue::NULL).await;

                    if config.use_translation {
                        if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                            if let Ok(status) = serde_wasm_bindgen::from_value::<FolderStatus>(st) {
                                if status.exists {
                                    add_system_log(
                                        "info",
                                        "UI",
                                        "Starting AI translation engine...",
                                    );
                                    set_model_ready.set(true);
                                    set_status_text.set("AI Engine Starting...".to_string());
                                } else {
                                    add_system_log(
                                        "warn",
                                        "Sidecar",
                                        "Model missing. AI is disabled.",
                                    );
                                    set_model_ready.set(false);
                                }
                            }
                        }

                        if let Ok(st) = invoke("check_ai_server_status", JsValue::NULL).await {
                            if let Ok(status) = serde_wasm_bindgen::from_value::<FolderStatus>(st) {
                                if status.exists {
                                    add_system_log(
                                        "info",
                                        "UI",
                                        "Starting AI translation engine...",
                                    );
                                    let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
                                    set_model_ready.set(true);
                                    set_status_text.set("AI Engine Starting...".to_string());
                                    if let Ok(st) =
                                        invoke("ai_server_health_check", JsValue::NULL).await
                                    {
                                        let payload = st.as_bool().unwrap();
                                        if payload {
                                            signals.set_translator_state.set("Active".to_string());
                                        }
                                    }
                                } else {
                                    add_system_log(
                                        "warn",
                                        "Sidecar",
                                        "AI Server missing. AI is disabled.",
                                    );
                                    set_model_ready.set(false);
                                }
                            }
                        }

                        if let Ok(res) = invoke("check_dict_update", JsValue::NULL).await {
                            if let Some(needed) = res.as_bool() {
                                set_dict_update_available.set(needed);
                            }
                        }
                    }

                    if config.always_on_top {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                            "onTop": true
                        }))
                        .unwrap();
                        let _ = invoke("set_always_on_top", args).await;
                    }

                    add_system_log("info", "Updater", "Checking for remote updates...");
                    match invoke("check_all_updates", JsValue::NULL).await {
                        Ok(update_res) => {
                            if let Ok(update_data) = serde_wasm_bindgen::from_value::<
                                crate::ui_types::UpdateCheckResult,
                            >(update_res)
                            {
                                log!("data {:?}", update_data);

                                // 1. Silent Dictionary Update
                                if update_data.dict_update_available {
                                    add_system_log(
                                        "info",
                                        "Updater",
                                        "New dictionary found. Applying silently...",
                                    );
                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                            "version": update_data.remote_data.dictionary.version.clone()
                                        }))
                                        .unwrap();
                                    let _ = invoke("sync_dictionary", args).await;
                                }

                                // 2. Save metadata for the modals to use
                                set_pending_update_data.set(Some(update_data.remote_data.clone()));

                                // 3. Trigger Popups
                                if update_data.app_update_available {
                                    set_show_app_update_modal.set(true);
                                }
                                if update_data.model_update_available && config.use_translation {
                                    set_show_model_update_modal.set(true);
                                }
                            }
                        }
                        Err(e) => {
                            log!("FATAL: check_all_updates failed: {:?}", e);
                            add_system_log("error", "Updater", &format!("Check failed: {:?}", e));
                        }
                    }

                    set_status_text.set("Ready".to_string());
                } else {
                    log!("New user detected. Showing Wizard.");
                    add_system_log("info", "Setup", "Awaiting initial configuration.");
                }
            }
        }
        Err(e) => log!("FATAL: Failed to load config: {:?}", e),
    }
}
