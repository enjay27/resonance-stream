use futures::FutureExt;
use indexmap::IndexMap;
use leptos::html;
use leptos::leptos_dom::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;
use web_sys::HtmlDivElement;
use crate::components::{ChatContainer, ChatRow, NavBar};
use crate::components::settings::Settings;
use crate::components::title_bar::TitleBar;
use crate::hooks::use_config::save_app_config;
use crate::hooks::use_events::setup_event_listeners;
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::{invoke, listen};
use crate::types::{
    ChatMessage, SystemMessage, AppConfig, ModelStatus, ProgressPayload, TauriEvent
};
use crate::utils::{add_system_log, copy_to_clipboard, format_time, is_japanese};

#[component]
pub fn App() -> impl IntoView {
    // --- STATE SIGNALS ---
    let (init_done, set_init_done) = signal(false); // Hydrated from config
    let (use_translation, set_use_translation) = signal(false);
    let (compute_mode, set_compute_mode) = signal("cpu".to_string());
    let (wizard_step, set_wizard_step) = signal(0); // 0: Welcome, 1: Options, 2: Download

    let (is_translator_active, set_is_translator_active) = signal(false);
    let (is_sniffer_active, set_is_sniffer_active) = signal(false);
    let (status_text, set_status_text) = signal("".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);

    let (active_tab, set_active_tab) = signal("전체".to_string());
    let (search_term, set_search_term) = signal("".to_string());
    let (name_cache, set_name_cache) = signal(std::collections::HashMap::<String, String>::new());
    let (chat_log, set_chat_log) = signal(IndexMap::<u64, RwSignal<ChatMessage>>::new());
    let (system_log, set_system_log) = signal(Vec::<RwSignal<SystemMessage>>::new());

    let (is_system_at_bottom, set_system_at_bottom) = signal(true);
    let (show_system_tab, set_show_system_tab) = signal(false);
    let (system_level_filter, set_system_level_filter) = signal(None::<String>);
    let (system_source_filter, set_system_source_filter) = signal(None::<String>);

    let (compact_mode, set_compact_mode) = signal(false);
    let (is_pinned, set_is_pinned) = signal(false);
    let (show_settings, set_show_settings) = signal(false);
    let (chat_limit, set_chat_limit) = signal(1000);
    let (custom_filters, set_custom_filters) = signal(vec!["WORLD".to_string(), "GUILD".to_string(), "PARTY".to_string(), "LOCAL".to_string()]);
    let (theme, set_theme) = signal("dark".to_string());
    let (opacity, set_opacity) = signal(0.85f32);
    let (is_debug, set_is_debug) = signal(false);
    let (tier, set_tier) = signal("middle".to_string());
    let (restart_required, set_restart_required) = signal(false);
    let (dict_update_available, set_dict_update_available) = signal(false);
    let (is_at_bottom, set_is_at_bottom) = signal(true);
    let (unread_count, set_unread_count) = signal(0);
    let (active_menu_id, set_active_menu_id) = signal(None::<u64>);
    let (archive_chat, set_archive_chat) = signal(false);

    let signals = AppSignals {
        init_done, set_init_done,
        use_translation, set_use_translation,
        compute_mode, set_compute_mode,
        wizard_step, set_wizard_step,
        is_translator_active, set_is_translator_active,
        is_sniffer_active, set_is_sniffer_active,
        status_text, set_status_text,
        model_ready, set_model_ready,
        downloading, set_downloading,
        progress, set_progress,
        active_tab, set_active_tab,
        search_term, set_search_term,
        name_cache, set_name_cache,
        chat_log, set_chat_log,
        system_log, set_system_log,
        is_system_at_bottom, set_system_at_bottom,
        show_system_tab, set_show_system_tab,
        system_level_filter, set_system_level_filter,
        system_source_filter, set_system_source_filter,
        compact_mode, set_compact_mode,
        is_pinned, set_is_pinned,
        show_settings, set_show_settings,
        chat_limit, set_chat_limit,
        custom_filters, set_custom_filters,
        theme, set_theme,
        opacity, set_opacity,
        is_debug, set_is_debug,
        tier, set_tier,
        restart_required, set_restart_required,
        dict_update_available, set_dict_update_available,
        is_at_bottom, set_is_at_bottom,
        unread_count, set_unread_count,
        active_menu_id, set_active_menu_id,
        archive_chat, set_archive_chat,
    };

    provide_context(signals);

    // --- WATCHDOG: SIDE CAR MONITOR ---
    spawn_local(async move {
        loop {
            if let Ok(res) = invoke("is_translator_running", JsValue::NULL).await {
                if let Some(running) = res.as_bool() {
                    set_is_translator_active.set(running);

                    if !running && use_translation.get_untracked() && init_done.get_untracked() {
                        add_system_log("Warning", "[WatchDog]", "Translator not running. Run Translator Sidecar.");
                        let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
                    }
                }
            }

            if let Ok(res) = invoke("is_sniffer_active", JsValue::NULL).await {
                if let Some(active) = res.as_bool() {
                    set_is_sniffer_active.set(active);

                    if !active && init_done.get_untracked() {
                        add_system_log("Warning", "[WatchDog]", "Sniffer not running. Run Sniffer.");
                        let _ = invoke("start_sniffer_command", JsValue::NULL).await;
                    }
                }
            }

            // Poll every 5 seconds
            gloo_timers::future::TimeoutFuture::new(5000).await;
        }
    });
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
            show_system_tab: show_system_tab.get_untracked(),
            is_debug: is_debug.get_untracked(),
            tier: tier.get_untracked(),
            archive_chat: archive_chat.get_untracked(),
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
                set_chat_log.set(IndexMap::new());
                set_system_log.set(Vec::new());
            }
        }.boxed_local()
    });

    let actions = AppActions { save_config, clear_history };

    provide_context(actions);

    // Apply theme to the root element whenever it changes
    Effect::new(move |_| {
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(body) = doc.body() {
                    let _ = body.set_attribute("data-theme", &theme.get());
                }
            }
        }
    });

    Effect::new(move |_| {
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(el) = doc.get_element_by_id("main-app-container") {
                    let _ = el.set_attribute("style", &format!("--overlay-opacity: {};", opacity.get()));
                }
            }
        }
    });

    // --- DICTIONARY SYNC ACTION ---
    let sync_dict_action = Action::new_local(|_: &()| async move {
        // We move the !Send Tauri future into this local action
        match invoke("sync_dictionary", JsValue::NULL).await {
            Ok(_) => {
                log!("Dictionary Synced Successfully");
                "최신 상태".to_string()
            }
            Err(e) => {
                log!("Sync Error: {:?}", e);
                "동기화 실패".to_string()
            }
        }
    });

    let sync_status = move || sync_dict_action.value().get().unwrap_or_else(|| "".to_string());
    let is_syncing = sync_dict_action.pending();

    // --- OPTIMIZED VIEW LOGIC ---
    let filtered_chat = Memo::new(move |_| {
        let tab = active_tab.get();
        let search = search_term.get().to_lowercase();
        let filters = custom_filters.get();

        // If viewing System, return empty here to avoid processing overhead
        if tab == "시스템" { return Vec::new(); }

        let base_list = match tab.as_str() {
            "전체" => chat_log.get().values().cloned().collect::<Vec<_>>(),
            "커스텀" => chat_log.get().values()
                .filter(|m| filters.contains(&m.get().channel))
                .cloned().collect(),
            _ => {
                let key = match tab.as_str() {
                    "로컬" => "LOCAL", "파티" => "PARTY", "길드" => "GUILD", _ => "WORLD"
                };
                chat_log.get().values()
                    .filter(|m| m.get().channel == key)
                    .cloned().collect()
            }
        };

        if search.is_empty() { base_list }
        else {
            base_list.into_iter().filter(|sig| {
                let m = sig.get();
                m.nickname.to_lowercase().contains(&search) || m.message.to_lowercase().contains(&search)
            }).collect()
        }
    });

    let filtered_system_logs = Memo::new(move |_| {
        let logs = system_log.get();
        let level_f = system_level_filter.get();
        let source_f = system_source_filter.get();
        let search = search_term.get().to_lowercase();
        let debug_enabled = is_debug.get();

        logs.into_iter().filter(|sig| {
            let m = sig.get();

            if !debug_enabled && m.level == "debug" { return false; }

            let matches_level = level_f.as_ref().map_or(true, |f| &m.level == f);
            let matches_source = source_f.as_ref().map_or(true, |f| &m.source == f);
            let matches_search = search.is_empty() || m.message.to_lowercase().contains(&search);

            matches_level && matches_source && matches_search
        }).collect::<Vec<_>>()
    });

    // 1. STATE: Track if the user is currently at the bottom

    let chat_container_ref = create_node_ref::<html::Div>();

    // 2. EFFECT: Auto-scroll when messages update
    Effect::new(move |_| {
        // We track 'filtered_messages' so this runs ONLY when the visible list changes
        filtered_chat.track();

        // [CRITICAL FIX] Only execute scroll logic if the user is ALREADY at the bottom.
        // If they have scrolled up (is_at_bottom is false), this entire block is ignored.
        if is_at_bottom.get_untracked() {
            request_animation_frame(move || {
                // Double-check after render to ensure the state is still 'at_bottom'
                if is_at_bottom.get_untracked() {
                    if let Some(el) = chat_container_ref.get() {
                        el.set_scroll_top(el.scroll_height());
                    }
                }
            });
        }
    });

    // Effect: Auto-scroll specifically for System Messages
    Effect::new(move |_| {
        // 1. Track the system_log signal
        system_log.track();

        // 2. Only scroll if the user is in the system tab and already at the bottom
        if active_tab.get_untracked() == "시스템" && is_system_at_bottom.get_untracked() {
            request_animation_frame(move || {
                if let Some(el) = chat_container_ref.get() {
                    el.set_scroll_top(el.scroll_height());
                }
            });
        }
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

            if use_translation.get_untracked() {
                add_system_log("info", "UI", "Starting AI translation engine...");
                // Check model one last time before launching AI
                if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                    if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(st) {
                        if status.exists {
                            let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
                            set_status_text.set("AI Engine Starting...".to_string());
                        }
                    }
                }
            }
        });
    };

    let start_download = move |ev: web_sys::MouseEvent| {
        // Prevent the default button behavior if necessary
        ev.prevent_default();

        set_downloading.set(true);
        set_status_text.set("Starting Download...".to_string());
        spawn_local(async move {
            let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(wrapper) = serde_wasm_bindgen::from_value::<TauriEvent>(event_obj) {
                    set_progress.set(wrapper.payload.total_percent);
                    set_status_text.set(format!("Downloading AI Model {}%", wrapper.payload.total_percent));
                }
            }) as Box<dyn FnMut(JsValue)>);
            let _ = listen("download-progress", &closure).await;
            match invoke("download_model", JsValue::NULL).await {
                Ok(_) => {
                    set_downloading.set(false);
                    set_model_ready.set(true);
                    set_status_text.set("Ready".to_string());
                    finalize_setup(());
                }
                Err(e) => {
                    set_downloading.set(false);
                    set_status_text.set(format!("Error: {:?}", e));
                    add_system_log("error", "ModelManager", &format!("Download failed: {:?}", e));
                }
            }
            closure.forget();
        });
    };

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
                        set_show_system_tab.set(config.show_system_tab);
                        set_is_debug.set(config.is_debug);
                        set_tier.set(config.tier);
                        set_archive_chat.set(config.archive_chat);

                        // 2. If the user hasn't finished the wizard, stop here
                        if config.init_done {
                            log!("Existing user detected. Auto-starting services.");
                            add_system_log("info", "Sniffer", "Auto-starting services...");
                            setup_event_listeners(signals).await;

                            // Hydrate GAME History
                            if let Ok(res) = invoke("get_chat_history", JsValue::NULL).await {
                                if let Ok(vec) = serde_wasm_bindgen::from_value::<Vec<ChatMessage>>(res) {
                                    let sanitized_vec: Vec<(u64, RwSignal<ChatMessage>)> = vec.into_iter().map(|mut p| {
                                        if p.message.starts_with("emojiPic=") { p.message = "스티커 전송".to_string(); } else if p.message.contains("<sprite=") { p.message = "이모지 전송".to_string(); }
                                        (p.pid, RwSignal::new(p))
                                    }).collect();
                                    set_chat_log.set(sanitized_vec.into_iter().collect());
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
                                    if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(st) {
                                        if status.exists {
                                            add_system_log("info", "UI", "Starting AI translation engine...");
                                            let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
                                            set_model_ready.set(true);
                                            set_status_text.set("AI Engine Starting...".to_string());
                                        } else {
                                            add_system_log("warn", "Sidecar", "Model missing. AI is disabled.");
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
                            })).unwrap();
                                let _ = invoke("set_always_on_top", args).await;
                            }

                            set_status_text.set("Ready".to_string());
                        } else {
                            log!("New user detected. Showing Wizard.");
                            add_system_log("info", "Setup", "Awaiting initial configuration.");
                        }
                    }
                },
                Err(e) => log!("FATAL: Failed to load config: {:?}", e),
            }
        });
    });

    view! {
        <main id="main-app-container" class=move || if compact_mode.get() { "chat-app compact" } else { "chat-app" }>
            <Show when=move || active_menu_id.get().is_some()>
                <div class="menu-overlay" on:click=move |_| set_active_menu_id.set(None)></div>
            </Show>
            <TitleBar />
            <Show when=move || init_done.get() fallback=move || view! {
                <div class="setup-view">
                    <div class="wizard-card">
                        {move || match wizard_step.get() {
                            0 => view! {
                                <div class="wizard-step">
                                    <h1>"Resonance Stream"</h1>
                                    <p>"블루 프로토콜의 게임 채팅을 실시간으로 분석하고 번역합니다."</p>
                                    <button class="primary-btn" on:click=move |_| set_wizard_step.set(1)>"시작하기"</button>
                                </div>
                            }.into_any(),
                            1 => view! {
                                <div class="wizard-step">
                                    <h2>"빠른 설정"</h2>
                                    <div class="setting-item">
                                        <label class="checkbox-row">
                                            <input type="checkbox" checked=move || use_translation.get() on:change=move |ev| set_use_translation.set(event_target_checked(&ev)) />
                                            <span>"실시간 번역 기능 활성화."</span>
                                            <p>"설정에서 바꿀 수 있습니다."</p>
                                        </label>
                                    </div>
                                    <Show when=move || use_translation.get()>
                                        <div class="setting-item">
                                            <h3>"연산 장치 (Compute Mode)"</h3>
                                            <div class="radio-group">
                                                <label class="radio-row">
                                                    <input type="radio" name="mode" value="cpu" checked=move || compute_mode.get() == "cpu" on:change=move |_| set_compute_mode.set("cpu".into()) />
                                                    <span>"CPU (가장 높은 호환성)"</span>
                                                </label>
                                                <label class="radio-row">
                                                    <input type="radio" name="mode" value="cuda" checked=move || compute_mode.get() == "cuda" on:change=move |_| set_compute_mode.set("cuda".into()) />
                                                    <span>"GPU (고성능, NVIDIA CUDA 필요)"</span>
                                                </label>
                                            </div>
                                        </div>
                                    </Show>
                                    <button class="primary-btn" on:click=move |_| { if use_translation.get_untracked() { set_wizard_step.set(2); } else { finalize_setup(()); } }>"Next"</button>
                                </div>
                            }.into_any(),
                            2 => view! {
                                <div class="wizard-step">
                                    <h2>"Model Installation"</h2>
                                    <p>"번역을 위해 약 1.3GB의 AI 모델 파일 다운로드가 필요합니다."</p>
                                    <Show when=move || downloading.get() fallback=move || view! { <button class="primary-btn" on:click=start_download>"다운로드 시작"</button> }>
                                        <div class="progress-bar"><div class="fill" style:width=move || format!("{}%", progress.get())></div></div>
                                        <div class="progress-label">{move || format!("{}%", progress.get())}</div>
                                    </Show>
                                </div>
                            }.into_any(),
                            _ => view! { <div></div> }.into_any(),
                        }}
                    </div>
                </div>
            }>
                <NavBar />

                <ChatContainer />
            </Show>

            // Settings Modal
            <Settings />

        </main>
    }
}