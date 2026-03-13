use crate::components::settings::Settings;
use crate::components::title_bar::TitleBar;
use crate::components::{
    ChatContainer, DictionaryModal, NavBar, SetupWizard, Troubleshooter,
};
use crate::hooks::use_config::save_app_config;
use crate::hooks::use_events::setup_event_listeners;
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::{invoke, listen};
use crate::ui_types::{
    AppConfig, ChatMessage, FolderStatus, ProgressPayload, SystemMessage, TauriEvent,
};
use crate::utils::add_system_log;
use futures::FutureExt;
use indexmap::IndexMap;
use leptos::leptos_dom::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    // --- STATE SIGNALS ---
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
    let (name_cache, set_name_cache) = signal(std::collections::HashMap::<String, String>::new());
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
    let (current_time, set_current_time) = signal(chrono::Local::now().timestamp_millis() as u64);
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

    let (chat_db, set_chat_db) = signal(HashMap::<u64, RwSignal<ChatMessage>>::new());
    let (tab_views, set_tab_views) = signal(HashMap::<String, std::collections::VecDeque<u64>>::new());

    let signals = AppSignals {
        init_done, set_init_done,
        use_translation, set_use_translation,
        compute_mode, set_compute_mode,
        wizard_step, set_wizard_step,
        translator_state, set_translator_state,
        translator_error, set_translator_error,
        is_sniffer_active, set_is_sniffer_active,
        status_text, set_status_text,
        model_ready, set_model_ready,
        downloading, set_downloading,
        progress, set_progress,
        active_tab, set_active_tab,
        search_term, set_search_term,
        name_cache, set_name_cache,
        system_log, set_system_log,
        is_system_at_bottom, set_system_at_bottom,
        debug_mode, set_debug_mode,
        log_level, set_log_level,
        system_level_filter, set_system_level_filter,
        system_source_filter, set_system_source_filter,
        compact_mode, set_compact_mode,
        is_pinned, set_is_pinned,
        show_settings, set_show_settings,
        chat_limit, set_chat_limit,
        custom_filters, set_custom_filters,
        theme, set_theme,
        opacity, set_opacity,
        tier, set_tier,
        restart_required, set_restart_required,
        dict_update_available, set_dict_update_available,
        is_at_bottom, set_is_at_bottom,
        unread_count, set_unread_count,
        active_menu_id, set_active_menu_id,
        archive_chat, set_archive_chat,
        hide_original_in_compact, set_hide_original_in_compact,
        network_interface, set_network_interface,
        click_through, set_click_through,
        drag_to_scroll, set_drag_to_scroll,
        sniffer_state, set_sniffer_state,
        sniffer_error, set_sniffer_error,
        alert_keywords, set_alert_keywords,
        alert_volume, set_alert_volume,
        emphasis_keywords, set_emphasis_keywords,
        use_relative_time, set_use_relative_time,
        current_time, set_current_time,
        font_size, set_font_size,
        hide_blocked_messages, set_hide_blocked_messages,
        blocked_users, set_blocked_users,
        min_sender_level, set_min_sender_level,
        app_update_step, set_app_update_step,
        app_update_progress, set_app_update_progress,
        model_update_step, set_model_update_step,
        model_update_progress, set_model_update_progress,
        show_app_update_modal, set_show_app_update_modal,
        show_model_update_modal, set_show_model_update_modal,
        pending_update_data, set_pending_update_data,
        auto_sync_latest_dict, set_auto_sync_latest_dict,
        show_dictionary, set_show_dictionary,
        unread_counts, set_unread_counts,
        tab_switch_modifier, set_tab_switch_modifier,
        tab_switch_key, set_tab_switch_key,
        show_troubleshooter, set_show_troubleshooter,
        chat_db, set_chat_db,
        tab_views, set_tab_views,
    };

    provide_context(signals);

    // --- CONFIG ACTIONS ---
    let save_config = Action::new_local(move |_: &()| {
        let config = AppConfig {
            init_done: init_done.get_untracked(),
            use_translation: use_translation.get_untracked(),
            compute_mode: compute_mode.get_untracked(),
            compact_mode: compact_mode.get_untracked(),
            always_on_top: is_pinned.get_untracked(),
            active_tab: active_tab.get_untracked(),
            chat_limit: chat_limit.get_untracked(),
            custom_tab_filters: custom_filters.get_untracked(),
            theme: theme.get_untracked(),
            overlay_opacity: opacity.get_untracked(),
            debug_mode: debug_mode.get_untracked(),
            log_level: log_level.get_untracked(),
            tier: tier.get_untracked(),
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
            tab_switch_key: tab_switch_key.get_untracked(),
        };

        async move {
            save_app_config(config).await;
        }
    });

    // Action: Clear Chat
    let clear_history = Action::new_local(move |_: &()| {
        let confirmed = window().confirm_with_message("Clear all chat history?").unwrap_or(false);
        async move {
            if confirmed {
                crate::hooks::use_events::clear_backend_history().await;
                set_chat_db.set(HashMap::new());
                set_tab_views.set(HashMap::new());
                set_system_log.set(Vec::new());
                set_unread_count.set(0);
                set_unread_counts.set(HashMap::new());
            }
        }.boxed_local()
    });

    let finalize_setup = move |_| {
        set_init_done.set(true);
        add_system_log("success", "Setup", "Initial configuration completed.");
        save_config.dispatch(());

        spawn_local(async move {
            add_system_log("info", "Sniffer", "Initializing packet capture...");
            setup_event_listeners(signals).await;
            set_is_sniffer_active.set(true);
            let _ = invoke("start_sniffer_command", JsValue::NULL).await;
        });
    };

    let start_download = move |ev: web_sys::MouseEvent| {
        // Prevent the default button behavior if necessary
        ev.prevent_default();

        set_downloading.set(true);
        set_status_text.set("Starting Downloads...".to_string());

        spawn_local(async move {
            // FETCH THE GIST METADATA FIRST
            match invoke("check_all_updates", JsValue::NULL).await {
                Ok(config) => {
                    let config = serde_wasm_bindgen::from_value::<crate::ui_types::UpdateCheckResult>(config);
                }
                Err(err) => {
                    add_system_log("info", "downloader", &format!("{:?}", err));
                }
            }

            let update_res = invoke("check_all_updates", JsValue::NULL).await;
            let (model_url, model_version, model_hash, dict_version) = if let Ok(res) = update_res {
                if let Ok(data) =
                    serde_wasm_bindgen::from_value::<crate::ui_types::UpdateCheckResult>(res)
                {
                    (
                        data.remote_data.model.download_url,
                        data.remote_data.model.latest_version,
                        data.remote_data.model.sha256,
                        data.remote_data.dictionary.version,
                    )
                } else {
                    set_status_text.set("Error: Failed to parse update data".to_string());
                    set_downloading.set(false);
                    return;
                }
            } else {
                set_status_text.set("Error: Network check failed".to_string());
                set_downloading.set(false);
                return;
            };

            // 1. Setup the progress listener
            let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(wrapper) = serde_wasm_bindgen::from_value::<TauriEvent>(event_obj) {
                    set_progress.set(wrapper.payload.total_percent);
                    // Optional: Update status text to show what is currently downloading
                    // using the `current_file` field we defined in downloader.rs
                    set_status_text.set(format!(
                        "{} ({}%)",
                        wrapper.payload.current_file, wrapper.payload.total_percent
                    ));
                }
            }) as Box<dyn FnMut(JsValue)>);
            let _ = listen("download-progress", &closure).await;

            // 2. Download the AI Model (.gguf)
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "downloadUrl": model_url,
                "version": model_version,
                "expectedHash": model_hash
            }))
            .unwrap();

            let model_result = invoke("download_model", args).await;
            if let Err(e) = model_result {
                set_downloading.set(false);
                set_status_text.set(format!("Model Error: {:?}", e));
                add_system_log(
                    "error",
                    "ModelManager",
                    &format!("Model download failed: {:?}", e),
                );
                closure.forget();
                return;
            }

            // 3. Download the AI Server (llama-server.exe via zip)
            let server_result = invoke("download_ai_server", JsValue::NULL).await;
            if let Err(e) = server_result {
                set_downloading.set(false);
                set_status_text.set(format!("Server Error: {:?}", e));
                add_system_log(
                    "error",
                    "ModelManager",
                    &format!("Server download failed: {:?}", e),
                );
                closure.forget();
                return;
            }

            // 4. Sync dictionary
            let dict_args = serde_wasm_bindgen::to_value(&serde_json::json!({
                    "version": dict_version
                }))
                .unwrap();

            let sync_dict = invoke("sync_dictionary", dict_args).await;
            if let Err(e) = sync_dict {
                set_downloading.set(false);
                set_status_text.set(format!("Dict Error: {:?}", e));
                add_system_log(
                    "error",
                    "ModelManager",
                    &format!("Sync dictionary failed: {:?}", e),
                );
                closure.forget();
                return;
            }

            let _ = invoke("launch_translator", JsValue::NULL).await;

            // 5. Both downloads succeeded
            set_downloading.set(false);
            set_model_ready.set(true);
            set_status_text.set("Ready".to_string());
            finalize_setup(());

            closure.forget();
        });
    };

    // --- APP UPDATE LOGIC ---
    let start_app_update = move |download_url: String| {
        set_app_update_step.set(1);
        set_app_update_progress.set(0);

        spawn_local(async move {
            let progress_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(payload) =
                        serde_json::from_value::<ProgressPayload>(ev["payload"].clone())
                    {
                        if payload.current_file.contains("앱 업데이트") {
                            set_app_update_progress.set(payload.percent);
                            if payload.percent >= 100 {
                                set_app_update_step.set(2);
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("download-progress", &progress_closure).await;
            progress_closure.forget(); // Keep alive during download

            let args =
                serde_wasm_bindgen::to_value(&serde_json::json!({ "downloadUrl": download_url }))
                    .unwrap();
            let _ = invoke("download_app_update", args).await;
        });
    };

    // --- MODEL UPDATE LOGIC ---
    let start_model_update = move |download_url: String, version: String, expected_hash: String| {
        set_model_update_step.set(1);
        set_model_update_progress.set(0);

        spawn_local(async move {
            let progress_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(payload) =
                        serde_json::from_value::<ProgressPayload>(ev["payload"].clone())
                    {
                        if payload.current_file.contains("AI 모델") {
                            set_model_update_progress.set(payload.percent);
                            if payload.percent >= 100 {
                                set_model_update_step.set(2);
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("download-progress", &progress_closure).await;
            progress_closure.forget(); // Keep alive during download

            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "downloadUrl": download_url,
                "version": version,
                "expectedHash": expected_hash
            }))
            .unwrap();
            let _ = invoke("download_model", args).await;
        });
    };

    let actions = AppActions {
        save_config,
        clear_history,
    };

    provide_context(actions);

    // --- TRAY ICON LISTENER ---
    spawn_local(async move {
        // 1. Click-Through Listener (Existing)
        let tray_closure = Closure::wrap(Box::new(move |_: JsValue| {
            let current = click_through.get_untracked();
            set_click_through.set(!current);
            actions.save_config.dispatch(());

            spawn_local(async move {
                let _ = invoke(
                    "set_click_through",
                    serde_wasm_bindgen::to_value(&serde_json::json!({ "enabled": !current }))
                        .unwrap(),
                )
                .await;
            });
        }) as Box<dyn FnMut(JsValue)>);

        let _ = listen("tray-toggle-click-through", &tray_closure).await;
        tray_closure.forget();

        // 2. Always on Top Listener (NEW)
        let tray_top_closure = Closure::wrap(Box::new(move |_: JsValue| {
            let current = is_pinned.get_untracked();
            set_is_pinned.set(!current); // Flip the signal so the TitleBar icon updates!
            actions.save_config.dispatch(()); // Save to config file

            spawn_local(async move {
                // Tauri command expects { "onTop": bool }
                let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "onTop": !current }))
                    .unwrap();
                let _ = invoke("set_always_on_top", args).await;
            });
        }) as Box<dyn FnMut(JsValue)>);

        let _ = listen("tray-toggle-always-on-top", &tray_top_closure).await;
        tray_top_closure.forget();
    });

    // Apply theme to the root element whenever it changes
    Effect::new(move |_| {
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                // 1. Apply theme to <html> (DaisyUI standard) and FORCE transparency
                if let Some(html) = doc.document_element() {
                    let _ = html.set_attribute("data-theme", &theme.get());
                    // This strips DaisyUI's solid background so the Tauri window is clear
                    let _ =
                        html.set_attribute("style", "background-color: transparent !important;");
                }

                // 2. Ensure <body> is also fully transparent
                if let Some(body) = doc.body() {
                    let _ =
                        body.set_attribute("style", "background-color: transparent !important;");
                }
            }
        }
    });

    // --- STARTUP HYDRATION ---
    Effect::new(move |_| {
        spawn_local(async move {
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
                                log!("Res : {:?}", res);
                                match serde_wasm_bindgen::from_value::<Vec<ChatMessage>>(res) {
                                    Ok(vec) => {
                                        log!("Successfully loaded {} history messages", vec.len());
                                        let mut db = std::collections::HashMap::new();
                                        let mut tabs = std::collections::HashMap::<String, std::collections::VecDeque<u64>>::new();
                                        let limit = config.chat_limit;
                                        let filters = custom_filters.get_untracked();

                                        for mut p in vec {
                                            if p.message.starts_with("emojiPic=") { p.message = "[스티커]".to_string(); }
                                            else if p.message.contains("<sprite=") {
                                                let mut output = String::with_capacity(p.message.len());
                                                let mut current = p.message.as_str();
                                                while let Some(start) = current.find("<sprite=") {
                                                    output.push_str(&current[..start]);
                                                    if let Some(end) = current[start..].find('>') {
                                                        output.push_str("[이모지]");
                                                        current = &current[start + end + 1..];
                                                    } else { output.push_str(&current[start..]); current = ""; break; }
                                                }
                                                output.push_str(current); p.message = output;
                                            }

                                            let pid = p.pid;
                                            let ch = p.channel.clone();
                                            db.insert(pid, RwSignal::new(p));

                                            // Map to Tabs
                                            let all_tab = tabs.entry("전체".to_string()).or_insert_with(std::collections::VecDeque::new);
                                            all_tab.push_back(pid);
                                            if all_tab.len() > limit { all_tab.pop_front(); }

                                            let spec_tab = tabs.entry(ch.clone()).or_insert_with(std::collections::VecDeque::new);
                                            spec_tab.push_back(pid);
                                            if spec_tab.len() > limit { spec_tab.pop_front(); }

                                            if filters.contains(&ch) {
                                                let custom_tab = tabs.entry("커스텀".to_string()).or_insert_with(std::collections::VecDeque::new);
                                                custom_tab.push_back(pid);
                                                if custom_tab.len() > limit { custom_tab.pop_front(); }
                                            }
                                        }
                                        db.retain(|db_pid, _| tabs.values().any(|pid_list| pid_list.contains(db_pid)));
                                        set_chat_db.set(db);
                                        set_tab_views.set(tabs);
                                    }
                                    Err(e) => {
                                        // THIS WILL NOW PRINT THE EXACT ERROR!
                                        log!("❌ GAME HISTORY DESERIALIZATION ERROR: {:?}", e);
                                    }
                                }
                            }

                            // Hydrate SYSTEM History
                            if let Ok(res) = invoke("get_system_history", JsValue::NULL).await {
                                match serde_wasm_bindgen::from_value::<Vec<SystemMessage>>(res) {
                                    Ok(vec) => {
                                        set_system_log.set(vec.into_iter().map(|p| RwSignal::new(p)).collect());
                                    }
                                    Err(e) => {
                                        log!("❌ SYSTEM HISTORY DESERIALIZATION ERROR: {:?}", e);
                                    }
                                }
                            }

                            set_is_sniffer_active.set(true);
                            let _ = invoke("start_sniffer_command", JsValue::NULL).await;

                            if config.use_translation {
                                if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                                    if let Ok(status) =
                                        serde_wasm_bindgen::from_value::<FolderStatus>(st)
                                    {
                                        if status.exists {
                                            add_system_log(
                                                "info",
                                                "UI",
                                                "Starting AI translation engine...",
                                            );
                                            set_model_ready.set(true);
                                            set_status_text
                                                .set("AI Engine Starting...".to_string());
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

                                if let Ok(st) =
                                    invoke("check_ai_server_status", JsValue::NULL).await
                                {
                                    if let Ok(status) =
                                        serde_wasm_bindgen::from_value::<FolderStatus>(st)
                                    {
                                        if status.exists {
                                            add_system_log(
                                                "info",
                                                "UI",
                                                "Starting AI translation engine...",
                                            );
                                            let _ =
                                                invoke("start_translator_sidecar", JsValue::NULL)
                                                    .await;
                                            set_model_ready.set(true);
                                            set_status_text
                                                .set("AI Engine Starting...".to_string());
                                            if let Ok(st) =
                                                invoke("ai_server_health_check", JsValue::NULL)
                                                    .await
                                            {
                                                let payload = st.as_bool().unwrap();
                                                if payload {
                                                    signals
                                                        .set_translator_state
                                                        .set("Active".to_string());
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
                                    if let Ok(update_data) =
                                        serde_wasm_bindgen::from_value::<
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
                                        set_pending_update_data
                                            .set(Some(update_data.remote_data.clone()));

                                        // 3. Trigger Popups
                                        if update_data.app_update_available {
                                            set_show_app_update_modal.set(true);
                                        }
                                        if update_data.model_update_available
                                            && config.use_translation {
                                            set_show_model_update_modal.set(true);
                                        }
                                    }
                                }
                                Err(e) => {
                                    log!("FATAL: check_all_updates failed: {:?}", e);
                                    add_system_log(
                                        "error",
                                        "Updater",
                                        &format!("Check failed: {:?}", e),
                                    );
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
        });
    });

    // This automatically runs on startup, AND anytime either variable is changed from anywhere!
    Effect::new(move |_| {
        let ct = click_through.get();
        let aot = is_pinned.get();

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "clickThrough": ct,
                "alwaysOnTop": aot
            }))
            .unwrap();

            let _ = invoke("update_tray_menu", args).await;
        });
    });

    // --- TICKER FOR RELATIVE TIME ---
    // Updates the global current_time signal every 10 seconds
    spawn_local(async move {
        loop {
            set_current_time.set(chrono::Local::now().timestamp_millis() as u64);
            gloo_timers::future::TimeoutFuture::new(10_000).await;
        }
    });

    view! {
        <main id="main-app-container"
            class=move || if compact_mode.get() {
                "chat-app compact flex flex-col h-screen overflow-hidden"
            } else {
                "chat-app flex flex-col h-screen overflow-hidden"
            }
            // Natively binds your opacity signal to the DaisyUI theme background
            // style:background-color=move || {
            style=move || {
                let current_opacity = opacity.get();
                if theme.get() == "dark" {
                    format!("background-color: rgba(18, 18, 18, {}) !important;", current_opacity)
                } else {
                    format!("background-color: rgba(252, 252, 252, {}) !important;", current_opacity)
                }
            }
            // Note: Use `signals.opacity.get()` if your app.rs uses the signals struct instead of local signals.
        >
            <Show when=move || active_menu_id.get().is_some()>
                <div class="menu-overlay" on:click=move |_| set_active_menu_id.set(None)></div>
            </Show>
            <Show when=move || !compact_mode.get()>
                <TitleBar />
            </Show>
            <Show
                when=move || signals.init_done.get()
                fallback=move || view! {
                    <SetupWizard
                        finalize=Callback::new(finalize_setup)
                        start_download=Callback::new(start_download)
                    />
                }
            >
                <NavBar />

                <ChatContainer />
            </Show>

            // ==========================================
            // APP UPDATE MODAL
            // ==========================================
            <Show when=move || show_app_update_modal.get()>
                <div class="modal modal-open backdrop-blur-sm z-[30000]">
                    <div class="modal-box bg-base-300 border border-success/30">
                        <h3 class="font-black text-lg text-success mb-4">"새로운 앱 업데이트 가능!"</h3>
                        {move || pending_update_data.get().map(|data| view! {
                            <div class="space-y-4">
                                {move || match app_update_step.get() {
                                    0 => view! {
                                        <div class="animate-in fade-in">
                                            <p class="text-sm font-bold">"버전: " {data.app.latest_version.clone()}</p>
                                            <div class="bg-base-200 p-3 rounded text-xs opacity-80 whitespace-pre-wrap mt-2">
                                                {data.app.release_notes.clone()}
                                            </div>
                                            <div class="modal-action">
                                                <button class="btn btn-ghost text-base-content/50"
                                                    on:click={
                                                        // CLONE DATA BEFORE THE CLOSURE
                                                        let version = data.app.latest_version.clone();
                                                        move |_| {
                                                            let v = version.clone();
                                                            spawn_local(async move {
                                                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                    "target": "app",
                                                                    "version": v
                                                                })).unwrap();
                                                                let _ = invoke("ignore_update", args).await;
                                                            });
                                                            set_show_app_update_modal.set(false);
                                                        }
                                                    }>
                                                    "이번 버전 건너뛰기"
                                                </button>
                                                <button class="btn btn-success"
                                                    on:click={
                                                        // CLONE DATA BEFORE THE CLOSURE
                                                        let url = data.app.download_url.clone();
                                                        move |_| { start_app_update(url.clone()); }
                                                    }>
                                                    "다운로드 시작"
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any(),

                                    1 => view! {
                                        <div class="space-y-2 py-4 animate-in fade-in text-center">
                                            <p class="text-sm font-bold opacity-80">"업데이트 파일을 다운로드 중입니다..."</p>
                                            <progress class="progress progress-success w-full h-4" value=move || app_update_progress.get().to_string() max="100"></progress>
                                            <span class="text-xs font-mono">{move || format!("{}%", app_update_progress.get())}</span>
                                        </div>
                                    }.into_any(),

                                    _ => view! {
                                        <div class="space-y-4 py-4 animate-in zoom-in text-center">
                                            <div class="text-4xl mb-2">"🎉"</div>
                                            <p class="text-lg font-bold text-success">"다운로드 완료!"</p>
                                            <p class="text-xs opacity-70">"새로운 버전을 적용하려면 앱을 재시작해야 합니다."</p>
                                            <button class="btn btn-success btn-block mt-4 gap-2"
                                                on:click=move |_| {
                                                    set_status_text.set("재시작 중...".to_string());
                                                    spawn_local(async move { let _ = invoke("restart_to_apply_update", JsValue::NULL).await; });
                                                }>
                                                "재시작 및 적용"
                                            </button>
                                        </div>
                                    }.into_any(),
                                }}
                            </div>
                        })}
                    </div>
                </div>
            </Show>

            // ==========================================
            // MODEL UPDATE MODAL
            // ==========================================
            <Show when=move || show_model_update_modal.get()>
                <div class="modal modal-open backdrop-blur-sm z-[30000]">
                    <div class="modal-box bg-base-300 border border-info/30">
                        <h3 class="font-black text-lg text-info mb-4">"새로운 AI 번역 모델!"</h3>
                        {move || pending_update_data.get().map(|data| view! {
                            <div class="space-y-4">
                                {move || match model_update_step.get() {
                                    0 => view! {
                                        <div class="animate-in fade-in">
                                            <p class="text-sm font-bold">"버전: " {data.model.latest_version.clone()}</p>
                                            <div class="bg-base-200 p-3 rounded text-xs opacity-80 whitespace-pre-wrap mt-2">
                                                {data.model.release_notes.clone()}
                                            </div>
                                            <div class="modal-action">
                                                <button class="btn btn-ghost text-base-content/50"
                                                    on:click={
                                                        // CLONE DATA BEFORE THE CLOSURE
                                                        let version = data.model.latest_version.clone();
                                                        move |_| {
                                                            let v = version.clone();
                                                            spawn_local(async move {
                                                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                    "target": "model",
                                                                    "version": v
                                                                })).unwrap();
                                                                let _ = invoke("ignore_update", args).await;
                                                            });
                                                            set_show_model_update_modal.set(false);
                                                        }
                                                    }>
                                                    "건너뛰기"
                                                </button>
                                                <button class="btn btn-info"
                                                    on:click={
                                                        // CLONE DATA BEFORE THE CLOSURE
                                                        let url = data.model.download_url.clone();
                                                        let version = data.model.latest_version.clone();
                                                        let hash = data.model.sha256.clone();
                                                        move |_| {
                                                            start_model_update(url.clone(), version.clone(), hash.clone());
                                                        }
                                                    }>
                                                    "다운로드 시작 (약 2.4GB)"
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any(),

                                    1 => view! {
                                        <div class="space-y-2 py-4 animate-in fade-in text-center">
                                            <p class="text-sm font-bold opacity-80">"AI 모델을 다운로드 중입니다..."</p>
                                            <progress class="progress progress-info w-full h-4" value=move || model_update_progress.get().to_string() max="100"></progress>
                                            <span class="text-xs font-mono">{move || format!("{}%", model_update_progress.get())}</span>
                                        </div>
                                    }.into_any(),

                                    _ => view! {
                                        <div class="space-y-4 py-4 animate-in zoom-in text-center">
                                            <div class="text-4xl mb-2">"✨"</div>
                                            <p class="text-lg font-bold text-info">"모델 다운로드 완료!"</p>
                                            <p class="text-xs opacity-70">"새로운 AI 모델이 디스크에 성공적으로 저장되었습니다."</p>
                                            <button class="btn btn-info btn-block mt-4 gap-2"
                                                on:click=move |_| {
                                                    set_show_model_update_modal.set(false);
                                                    signals.set_restart_required.set(true);
                                                }>
                                                "확인"
                                            </button>
                                        </div>
                                    }.into_any(),
                                }}
                            </div>
                        })}
                    </div>
                </div>
            </Show>

            <Troubleshooter />

            // Dictionary Modal
            <DictionaryModal />

            // Settings Modal
            <Settings />

        </main>
    }
}
