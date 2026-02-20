use indexmap::IndexMap;
use leptos::html;
use leptos::leptos_dom::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::HtmlDivElement;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn invoke_string(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    async fn listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;
}

// --- DATA STRUCTURES ---

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatPacket {
    pub pid: u64,
    pub channel: String,
    pub entity_id: u64,
    pub uid: u64,
    pub nickname: String,
    pub class_id: u64,
    pub status_flag: u64,
    pub level: u64,
    pub timestamp: u64,
    pub message: String,
    #[serde(default)]
    pub translated: Option<String>,
    #[serde(default)]
    pub nickname_romaji: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemMessage {
    pub pid: u64,
    pub timestamp: u64,
    pub level: String,  // info, warn, error, success, debug
    pub source: String, // Backend, Sniffer, Sidecar
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct AppConfig {
    init_done: bool,
    use_translation: bool,
    compute_mode: String,
    compact_mode: bool,
    always_on_top: bool,
    active_tab: String,
    chat_limit: usize,
    custom_tab_filters: Vec<String>,
    theme: String,
    overlay_opacity: f32,
    show_system_tab: bool,
    is_debug: bool,
    tier: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ModelStatus { exists: bool, path: String }

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TauriEvent { payload: ProgressPayload }

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProgressPayload { current: f64, total: f64, percent: u8 }

#[component]
pub fn App() -> impl IntoView {
    // --- STATE SIGNALS ---
    let (init_done, set_init_done) = signal(false); // Hydrated from config
    let (use_translation, set_use_translation) = signal(false);
    let (compute_mode, set_compute_mode) = signal("cpu".to_string());
    let (wizard_step, set_wizard_step) = signal(0); // 0: Welcome, 1: Options, 2: Download

    let (is_translator_active, set_is_translator_active) = signal(false);
    let (status_text, set_status_text) = signal("Initializing...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);

    let (active_tab, set_active_tab) = signal("전체".to_string());
    let (search_term, set_search_term) = signal("".to_string());
    let (name_cache, set_name_cache) = signal(std::collections::HashMap::<String, String>::new());
    let (chat_log, set_chat_log) = signal(IndexMap::<u64, RwSignal<ChatPacket>>::new());
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

    // --- HELPERS ---
    let add_system_log = move |level: &str, source: &str, message: &str| {
        let msg_json = serde_json::json!({
        "level": level,
        "source": source,
        "message": message
    });

        spawn_local(async move {
            // This triggers the backend which emits 'system-event'
            // that your existing listener already handles
            let _ = invoke("inject_system_message", serde_wasm_bindgen::to_value(&msg_json).unwrap()).await;
        });
    };

    let format_time = |ts: u64| {
        let date = js_sys::Date::new(&JsValue::from_f64(ts as f64 * 1000.0));
        format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
    };

    let is_japanese = |text: &str| {
        let re = js_sys::RegExp::new("[\\u3040-\\u309F\\u30A0-\\u30FF\\u4E00-\\u9FAF]", "");
        re.test(text)
    };

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
            // Poll every 3 seconds
            gloo_timers::future::TimeoutFuture::new(5000).await;
        }
    });

    // Copy Action
    let copy_text = move |text: String| {
        spawn_local(async move {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                // This requires "Clipboard" feature in web-sys (usually enabled by default in Tauri templates)
                let _ = navigator.clipboard().write_text(&text);
            }
        });
    };

    // --- CONFIG ACTIONS ---
    let save_config_action = Action::new_local(move |_: &()| {
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
        };

        async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "config": config })).unwrap();
            let _ = invoke("save_config", args).await;
        }
    });

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

    // --- FINE-GRAINED REACTIVE STATE ---
    // The IndexMap now holds individual RwSignals for each message.
    // let (chat_log, set_chat_log) = signal(IndexMap::<u64, RwSignal<ChatPacket>>::new());

    let (is_user_scrolling, set_user_scrolling) = signal(false);
    let chat_container_ref = create_node_ref::<html::Div>();

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

    // --- ACTIONS ---
    let setup_listeners = move || {
        spawn_local(async move {
            // --- LISTENER: NICKNAME UPDATES ---
            let nick_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    let payload = &ev["payload"];
                    let nickname = payload["nickname"].as_str().unwrap_or_default().to_string();
                    let romaji = payload["romaji"].as_str().unwrap_or_default().to_string();
                    let target_pid = payload["pid"].as_u64().unwrap_or(0);

                    // Update Cache for future lookups
                    set_name_cache.update(|cache| { cache.insert(nickname, romaji.clone()); });

                    // Update the specific message that triggered this
                    chat_log.with_untracked(|log| {
                        if let Some(sig) = log.get(&target_pid) {
                            sig.update(|p| p.nickname_romaji = Some(romaji.clone()));
                        }
                    });
                }
            }) as Box<dyn FnMut(JsValue)>);

            // --- LISTENER: MESSAGE TRANSLATIONS ---
            let msg_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    let resp = &ev["payload"];
                    let target_pid = resp["pid"].as_u64().unwrap_or(0);
                    let text = resp["translated"].as_str().unwrap_or_default().to_string();

                    if target_pid > 0 {
                        chat_log.with_untracked(|log| {
                            if let Some(sig) = log.get(&target_pid) {
                                sig.update(|p| p.translated = Some(text));
                            }
                        });
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            // LISTENER 1: Game Packets
            let packet_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(mut packet) = serde_json::from_value::<ChatPacket>(ev["payload"].clone()) {

                        // [FIX] Ignore messages belonging to the SYSTEM channel in game tabs
                        if packet.channel == "SYSTEM" { return; }

                        // [NEW] Sticker Transformation Logic
                        // Detects "emojiPic=" pattern and replaces it with "스티커 전송"
                        if packet.message.starts_with("emojiPic=") {
                            packet.message = "스티커 전송".to_string();
                            // Optional: We can force translation to None since it's already "translated"
                            packet.translated = None;
                        }
                        if packet.message.starts_with("<sprite=") {
                            packet.message = "이모지 전송".to_string();
                            // Optional: We can force translation to None since it's already "translated"
                            packet.translated = None;
                        }

                        let packet_clone = packet.clone();
                        let packet_nickname = packet_clone.clone();

                        set_chat_log.update(|log| {
                            let limit = chat_limit.get_untracked();
                            while log.len() >= limit && !log.is_empty() {
                                log.shift_remove_index(0);
                            }
                            log.insert(packet.pid, RwSignal::new(packet.clone()));
                        });

                        let current_tab = active_tab.get_untracked();
                        let is_relevant = match current_tab.as_str() {
                            "전체" => true,
                            "월드" => packet_clone.channel == "WORLD",
                            "길드" => packet_clone.channel == "GUILD",
                            "파티" => packet_clone.channel == "PARTY",
                            "로컬" => packet_clone.channel == "LOCAL",
                            _ => false,
                        };

                        if is_relevant && !is_at_bottom.get_untracked() {
                            set_unread_count.update(|n| *n += 1);
                        }

                        let pid = packet.pid;
                        let nickname = packet.nickname.clone();

                        // NICKNAME STRATEGY: Check Cache -> Request if Missing
                        let cached_nickname = name_cache.with(|cache| cache.get(&nickname).cloned());

                        if let Some(romaji) = cached_nickname {
                            packet.nickname_romaji = Some(romaji);
                        } else if is_japanese(&nickname) {
                            // Request nickname-only romanization
                            spawn_local(async move {
                                let _ = invoke("translate_nickname", serde_wasm_bindgen::to_value(&serde_json::json!({
                                    "pid": pid, "nickname": nickname
                                })).unwrap()).await;
                            });
                        }

                        // MESSAGE STRATEGY: Request translation if Japanese
                        if is_japanese(&packet.message) {
                            spawn_local(async move {
                                let _ = invoke("translate_message", serde_wasm_bindgen::to_value(&serde_json::json!({
                                    "text": packet_nickname.message, "pid": pid, "nickname": None::<String>
                                })).unwrap()).await;
                            });
                        }

                        // Store in UI log
                        set_chat_log.update(|log| { log.insert(pid, RwSignal::new(packet.clone())); });
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            // LISTENER 2: System Logs (New!)
            let system_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    // Parse as SystemMessage
                    if let Ok(packet) = serde_json::from_value::<SystemMessage>(ev["payload"].clone()) {
                        set_system_log.update(|log| {
                            if log.len() >= 200 { log.remove(0); }
                            log.push(RwSignal::new(packet));
                        });
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("packet-event", &packet_closure).await;
            listen("system-event", &system_closure).await;
            listen("nickname-feature-event", &nick_closure).await;
            listen("translation-feature-event", &msg_closure).await;

            packet_closure.forget();
            system_closure.forget();
            nick_closure.forget();
            msg_closure.forget();
        });
    };

    let finalize_setup = move |_| {
        set_init_done.set(true);
        add_system_log("success", "Setup", "Initial configuration completed.");
        save_config_action.dispatch(());

        spawn_local(async move {
            add_system_log("info", "Sniffer", "Initializing packet capture...");
            setup_listeners();
            let _ = invoke("start_sniffer_command", JsValue::NULL).await;

            if use_translation.get_untracked() {
                add_system_log("info", "UI", "Starting AI translation engine...");
                // Check model one last time before launching AI
                if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                    if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(st) {
                        if status.exists {
                            let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
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
                    set_progress.set(wrapper.payload.percent);
                }
            }) as Box<dyn FnMut(JsValue)>);
            let _ = listen("download-progress", &closure).await;
            closure.forget();
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

                        // 2. If the user hasn't finished the wizard, stop here
                        if config.init_done {
                            log!("Existing user detected. Auto-starting services.");
                            add_system_log("info", "Sniffer", "Auto-starting services...");
                            setup_listeners();

                            // Hydrate GAME History
                            if let Ok(res) = invoke("get_chat_history", JsValue::NULL).await {
                                if let Ok(vec) = serde_wasm_bindgen::from_value::<Vec<ChatPacket>>(res) {
                                    let sanitized_vec: Vec<(u64, RwSignal<ChatPacket>)> = vec.into_iter().map(|mut p| {
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

                            let _ = invoke("start_sniffer_command", JsValue::NULL).await;

                            if config.use_translation {
                                if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                                    if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(st) {
                                        if status.exists {
                                            add_system_log("info", "UI", "Starting AI translation engine...");
                                            let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
                                            set_model_ready.set(true);
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

    // Action: Clear Chat
    let clear_chat = move |_| {
        // 1. Confirm with user (Optional but recommended)
        if !window().confirm_with_message("Clear all chat history?").unwrap_or(false) {
            return;
        }

        spawn_local(async move {
            // 2. Call Backend
            let _ = invoke("clear_chat_history", JsValue::NULL).await;

            // 3. Clear Frontend Signals immediately
            set_chat_log.set(IndexMap::new());
            set_system_log.set(Vec::new());
        });
    };

    view! {
        <main id="main-app-container" class=move || if compact_mode.get() { "chat-app compact" } else { "chat-app" }>
            <Show when=move || active_menu_id.get().is_some()>
                <div class="menu-overlay" on:click=move |_| set_active_menu_id.set(None)></div>
            </Show>
            <div class="custom-title-bar" data-tauri-drag-region>
                <div class="drag-handle" data-tauri-drag-region></div>
                <div class="window-title" style="pointer-events: none;">
                    "Resonance Stream"
                </div>
                <div class="window-controls">
                    <Show when=move || use_translation.get()>
                        <div class="status-dot-container title-bar-version"
                             class:online=move || is_translator_active.get()>
                             <span class="pulse-dot"></span>
                             <span>{move || if is_translator_active.get() { "번역 ON" } else { "번역 OFF" }}</span>
                        </div>
                    </Show>
                    <button class="win-btn" on:click=move |_| {
                        spawn_local(async move {
                            let _ = invoke("minimize_window", JsValue::NULL).await;
                        });
                    }>"—"</button>

                    <button class="win-btn close" on:click=move |_| {
                        spawn_local(async move {
                            let _ = invoke("close_window", JsValue::NULL).await;
                        });
                    }>"✕"</button>
                </div>
            </div>
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
                <nav class="tab-bar">
                    <div class="tabs">
                        <Show when=move || !compact_mode.get()
                            fallback=move || view! {
                                // Compact Header: Just show current tab name
                                <div class="compact-tab-indicator">
                                    <span class="indicator-dot" data-tab=move || active_tab.get()></span>
                                    <span class="indicator-text">{move || active_tab.get()}</span>
                                </div>
                            }
                        >
                            {move || {
                                let mut base_tabs = vec![
                                    ("전체", "♾️"),
                                    ("커스텀", "⭐"),
                                    ("월드", "🌐"),
                                    ("길드", "🛡️"),
                                    ("파티", "⚔️"),
                                    ("로컬", "📍"),
                                ];

                                // Conditionally add the System tab based on the signal
                                if show_system_tab.get() {
                                    base_tabs.push(("시스템", "⚙️"));
                                }

                                base_tabs.into_iter().map(|(full, short)| {
                                    let t_full = full.to_string();
                                    let t_click = t_full.clone();
                                    let t_data = t_full.clone();
                                    let t_tab = t_full.clone();

                                    view! {
                                        <button
                                            class=move || if active_tab.get() == t_tab { "tab-btn active" } else { "tab-btn" }
                                            data-tab=t_data
                                            on:click=move |_| {
                                                set_active_tab.set(t_click.clone());
                                                save_config_action.dispatch(());
                                            }
                                            title=t_full
                                        >
                                            <span class="tab-full">{full}</span>
                                            <span class="tab-short">{short}</span>
                                        </button>
                                    }
                                }).collect_view()
                            }}
                        </Show>
                    </div>

                    // --- DICTIONARY SYNC BUTTON ---
                    <div class="control-area">
                        <button class="icon-btn"
                            title=move || if compact_mode.get() { "Expand Mode" } else { "Compact Mode" }
                            on:click=move |_| {
                                set_compact_mode.update(|b| *b = !*b);
                                save_config_action.dispatch(()); // <--- TRIGGER SAVE
                            }
                        >
                            {move || if compact_mode.get() { "🔽" } else { "🔼" }}
                        </button>

                        // 1. Clear Chat Button
                        <button class="icon-btn danger"
                            title="Clear Chat History"
                            on:click=clear_chat
                        >
                            "🗑️"
                        </button>

                        <button
                            class=move || if is_pinned.get() { "icon-btn active-pin" } else { "icon-btn" }
                            title=move || if is_pinned.get() { "Unpin Window" } else { "Pin on Top" }
                            on:click=move |_| {
                                let new_state = !is_pinned.get();
                                set_is_pinned.set(new_state);

                                // Call Backend
                                spawn_local(async move {
                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                        "onTop": new_state
                                    })).unwrap();
                                    let _ = invoke("set_always_on_top", args).await;
                                });

                                // 2. Save to Config
                                save_config_action.dispatch(()); // <--- TRIGGER SAVE
                            }
                        >
                            // Rotate the pin slightly when active for visual flair
                            <span style=move || if is_pinned.get() { "transform: rotate(45deg); display:block;" } else { "" }>
                                "📌"
                            </span>
                        </button>

                        // 2. Sync Dictionary Button
                        <button class="sync-btn"
                            title="Update Dictionary"
                            on:click=move |_| {
                                sync_dict_action.dispatch(());
                                set_dict_update_available.set(false);
                            }
                            disabled=is_syncing
                        >
                            // Use a span to control Emoji size independently if needed
                            {move || if is_syncing.get() {
                                view! { <span style="font-size: 0.8rem">"동기화 중..."</span> }
                            } else {
                                // Emojis look better slightly larger
                                view! { <span style="font-size: 1.1rem; vertical-align: middle;">"📘🔁"</span> }
                            }}

                            <Show when=move || dict_update_available.get()>
                                <span class="update-dot"></span>
                            </Show>
                        </button>
                        <button class="icon-btn" on:click=move |_| set_show_settings.set(true)>
                            "⚙️"
                            <Show when=move || restart_required.get()>
                                <span class="restart-badge"></span>
                            </Show>
                        </button>
                    </div>
                </nav>

                <div class="chat-container"
                    node_ref=chat_container_ref
                    style="position: relative;"
                    // 4. SCROLL EVENT (Reset Logic)
                    on:scroll=move |ev| {
                        let el = event_target::<HtmlDivElement>(&ev);

                        // [CHANGED] Stricter tolerance (10px instead of 30px)
                        // This prevents "fighting" the scroll bar.
                        let at_bottom = el.scroll_height() - el.scroll_top() - el.client_height() < 10;

                        if active_tab.get_untracked() == "시스템" {
                            set_system_at_bottom.set(at_bottom);
                        } else {
                            set_is_at_bottom.set(at_bottom);
                            if at_bottom { set_unread_count.set(0); }
                        }
                    }
                >
                    // [NEW] Active Filter Chip
                    <Show when=move || !search_term.get().is_empty()>
                        <div class="filter-overlay-container">
                            <div class="filter-chip"
                                 data-filter-type=move || active_tab.get() // Pass tab name to CSS
                            >
                                <span class="filter-label">"Filtering: " {move || search_term.get()}</span>
                                <button class="filter-close-btn"
                                    on:click=move |_| set_search_term.set("".to_string())
                                >
                                    "✕"
                                </button>
                            </div>
                        </div>
                    </Show>

                    // 1. FILTER CHIPS (Inside chat-container)
                    <Show when=move || active_tab.get() == "시스템" && (system_level_filter.get().is_some() || system_source_filter.get().is_some())>
                        <div class="system-filter-toast">
                            <span class="filter-info">
                                "필터링: "
                                {move || system_source_filter.get().map(|s| format!("[{}] ", s.to_uppercase()))}
                                {move || system_level_filter.get().map(|l| l.to_uppercase())}
                            </span>
                            <button class="filter-reset-btn" on:click=move |_| {
                                set_system_level_filter.set(None);
                                set_system_source_filter.set(None);
                            }> "✕" </button>
                        </div>
                    </Show>

                    // [NEW] Top-Right Notification Badge
                    <Show when=move || { unread_count.get() > 0 }>
                        <div class="new-msg-toast"
                             data-filter-type=move || active_tab.get() // Apply same logic here
                             on:click=move |_| {
                                if let Some(el) = chat_container_ref.get() {
                                    el.set_scroll_top(el.scroll_height());
                                    set_is_at_bottom.set(true);
                                    set_unread_count.set(0);
                                }
                            }
                        >
                            {move || unread_count.get()} "개의 새로운 메세지"
                        </div>
                    </Show>

                    <Show
                        when=move || active_tab.get() == "시스템"
                        fallback=move || view! {
                            /* --- GAME CHAT LOOP (ChatPacket) --- */
                            <For
                                each=move || filtered_chat.get()
                                key=|sig| sig.get_untracked().pid
                                children=move |sig| {
                                    let msg = sig.get();
                                    let pid = msg.pid;
                                    let is_jp = is_japanese(&msg.message);
                                    let is_active = move || active_menu_id.get() == Some(pid);

                                    view! {
                                        <div class="chat-row" data-channel=move || sig.get().channel.clone()
                                             style:z-index=move || if is_active() { "10001" } else { "1" }>

                                            <div class="msg-header">
                                                // Restore Nickname Click & Active Class
                                                <span class=move || if search_term.get() == sig.get().nickname { "nickname active" } else { "nickname" }
                                                    on:click=move |ev| {
                                                        ev.stop_propagation();
                                                        if is_active() { set_active_menu_id.set(None); }
                                                        else { set_active_menu_id.set(Some(pid)); }
                                                    }
                                                >
                                                    {move || {
                                                        let p = sig.get();
                                                        match p.nickname_romaji {
                                                            Some(romaji) => format!("{}({})", p.nickname, romaji),
                                                            None => p.nickname.clone()
                                                        }
                                                    }}
                                                </span>

                                                // Restore Context Menu
                                                <Show when=is_active>
                                                    <div class="context-menu" on:click=move |ev| ev.stop_propagation()>
                                                        <button class="menu-item" on:click=move |_| {
                                                            copy_text(sig.get_untracked().nickname);
                                                            set_active_menu_id.set(None);
                                                        }>
                                                            <span class="menu-icon">"📋"</span>"Copy Name"
                                                        </button>
                                                        <button class="menu-item" on:click=move |_| {
                                                            let n = sig.get_untracked().nickname;
                                                            if search_term.get_untracked() == n { set_search_term.set("".into()); }
                                                            else { set_search_term.set(n); }
                                                            set_active_menu_id.set(None);
                                                        }>
                                                            <span class="menu-icon">"🔍"</span>"Filter Chat"
                                                        </button>
                                                    </div>
                                                </Show>

                                                <span class="lvl">"Lv." {move || sig.get().level}</span>
                                                <span class="time">{format_time(msg.timestamp)}</span>
                                            </div>

                                            <div class="msg-wrapper">
                                                <div class="msg-body" class:has-translation=move || sig.get().translated.is_some()>
                                                    // Restore [원문] and [번역] Labels
                                                    <div class="original">
                                                        {if is_jp { "[원문] " } else { "" }} {move || sig.get().message.clone()}
                                                    </div>
                                                    {move || sig.get().translated.clone().map(|text| view! {
                                                        <div class="translated">"[번역] " {text}</div>
                                                    })}
                                                </div>
                                                <button class="copy-btn" on:click=move |ev| {
                                                    ev.stop_propagation();
                                                    copy_text(sig.get().message.clone());
                                                }> "📋" </button>
                                            </div>
                                        </div>
                                    }
                                }
                            />
                        }
                    >
                        /* --- SYSTEM LOG LOOP (Zero Filtering) --- */
                        <For
                            each=move || filtered_system_logs.get()
                            key=|sig| sig.get_untracked().pid
                            children=move |sig| {
                                view! {
                                    <div class="chat-row system-log" data-level=move || sig.get().level.clone()>
                                        <div class="msg-header">
                                            // Click Level to Filter
                                            <span class="level-badge clickable"
                                                  on:click=move |_| set_system_level_filter.set(Some(sig.get_untracked().level))
                                            >
                                                {move || sig.get().level.to_uppercase()}
                                            </span>

                                            // Click Source to Filter
                                            <span class="source-tag clickable"
                                                  on:click=move |_| set_system_source_filter.set(Some(sig.get_untracked().source))
                                            >
                                                {move || sig.get().source.to_uppercase()}
                                            </span>

                                            <span class="time">{move || format_time(sig.get().timestamp)}</span>
                                        </div>
                                        <div class="msg-body">{move || sig.get().message.clone()}</div>
                                    </div>
                                }
                            }
                        />
                    </Show>

                    // 2. SCROLL LOCK TOAST (Visible ONLY when ON the 시스템 tab and scrolled up)
                    <Show when=move || active_tab.get() == "시스템" && !is_system_at_bottom.get()>
                        <div class="scroll-lock-toast-bottom"
                             on:click=move |_| {
                                if let Some(el) = chat_container_ref.get() {
                                    el.set_scroll_top(el.scroll_height());
                                    set_system_at_bottom.set(true);
                                }
                             }
                        >
                            "⬆️ Scroll Locked (Click to Resume)"
                        </div>
                    </Show>
                </div>
            </Show>

            // Settings Modal
            <Show when=move || show_settings.get()>
                <div class="settings-overlay" on:click=move |_| set_show_settings.set(false)>
                    // Event propagation stopped manually to fix the previous error
                    <div class="settings-modal" on:click=move |ev| ev.stop_propagation()>

                        // Header
                        <div class="settings-header">
                            <h2>"Settings"</h2>
                            <button class="close-btn" on:click=move |_| set_show_settings.set(false)>"✕"</button>
                        </div>

                        // Content (Cleaned up)
                        <div class="settings-content">
                            <div class="setting-group">
                                <h3>"AI Translation Features"</h3>
                                <div class="toggle-row">
                                    <span class="toggle-label">"실시간 번역 기능 사용"</span>
                                    <input type="checkbox"
                                        prop:checked=move || use_translation.get()
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            set_use_translation.set(checked);
                                            save_config_action.dispatch(()); // Persist choice

                                            if checked {
                                                add_system_log("info", "Settings", "번역 기능이 활성화되었습니다. 엔진을 시작합니다.");
                                                spawn_local(async move {
                                                    let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
                                                });
                                            } else {
                                                add_system_log("warn", "Settings", "번역 기능이 비활성화되었습니다. (재시작 권장)");
                                                set_restart_required.set(true);
                                            }
                                        }
                                    />
                                </div>

                                <Show when=move || use_translation.get()>
                                    <div class="setting-row">
                                        <span class="toggle-label">"연산 장치 (Compute Mode)"</span>
                                        <div class="radio-group-compact">
                                            <label class="radio-row">
                                                <input type="radio" name="mode-settings" value="cpu"
                                                    checked=move || compute_mode.get() == "cpu"
                                                    on:change=move |_| {
                                                        set_compute_mode.set("cpu".into());
                                                        save_config_action.dispatch(());
                                                        add_system_log("warn", "Settings", "CPU 모드로 설정되었습니다. 재시작 후 적용됩니다.");
                                                        set_restart_required.set(true);
                                                    }
                                                />
                                                <span>"CPU"</span>
                                            </label>
                                            <label class="radio-row">
                                                <input type="radio" name="mode-settings" value="cuda"
                                                    checked=move || compute_mode.get() == "cuda"
                                                    on:change=move |_| {
                                                        set_compute_mode.set("cuda".into());
                                                        save_config_action.dispatch(());
                                                        add_system_log("warn", "Settings", "GPU 모드로 설정되었습니다. 재시작 후 적용됩니다.");
                                                        set_restart_required.set(true);
                                                    }
                                                />
                                                <span>"GPU"</span>
                                            </label>
                                        </div>
                                    </div>
                                    <p class="hint">"GPU 사용을 위해서는 NVIDIA 그래픽카드 + CUDA Toolkit 이 필요합니다. 설치되어있지 않다면 CPU 사용을 추천합니다."</p>
                                    <div class="setting-row">
                                        <span class="toggle-label">"성능"</span>
                                        <div class="radio-group-compact">
                                            {vec!["low", "middle", "high", "extreme"].into_iter().map(|t| {
                                                let t_val = t.to_string();
                                                let t_val_tier = t.to_string();
                                                view! {
                                                    <label class="radio-row">
                                                        <input type="radio" name="tier"
                                                            checked=move || tier.get() == t_val
                                                            on:change=move |_| {
                                                                set_tier.set(t_val_tier.clone());
                                                                save_config_action.dispatch(()); // Persist choice

                                                                let msg = format!(
                                                                    "성능 티어가 '{}'(으)로 변경되었습니다.\n새로운 설정을 적용하려면 앱을 재시작해야 합니다.\n\n지금 바로 새로고침할까요?",
                                                                    t_val_tier.to_uppercase()
                                                                );

                                                                if window().confirm_with_message(&msg).unwrap_or(false) {
                                                                    let _ = window().location().reload(); // Immediate refresh
                                                                } else {
                                                                    // Log a warning in Korean in the System tab
                                                                    spawn_local(async move {
                                                                        let _ = invoke("inject_system_message", serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                            "level": "warn",
                                                                            "source": "Settings",
                                                                            "message": "새 성능 설정은 앱을 재시작한 후에 적용됩니다."
                                                                        })).unwrap()).await;
                                                                    });
                                                                    set_restart_required.set(true); // Show a persistent warning
                                                                }
                                                            }
                                                        />
                                                        <span class:tier-extreme=move || t == "extreme">{t.to_uppercase()}</span>
                                                    </label>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </div>
                                    <p class="hint">"번역 성능이 좋아지지만 번역 시간이 오래걸리고 자원을 더 많이 소모합니다. 번역에 걸리는 시간을 보고 조정해주세요."</p>
                                </Show>
                                <h3>"Overlay Settings"</h3>
                                <div class="setting-row">
                                    <span>"Background Opacity"</span>
                                    <div class="slider-container">
                                        <input type="range" min="0.1" max="1.0" step="0.05"
                                            prop:value=move || opacity.get().to_string()
                                            on:input=move |ev| {
                                                let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                                set_opacity.set(val);
                                                save_config_action.dispatch(()); // Persist value
                                            }
                                        />
                                        <span class="opacity-value">{move || format!("{:.0}%", opacity.get() * 100.0)}</span>
                                    </div>
                                </div>
                                <h3>"Display Settings"</h3>
                                <div class="toggle-row" on:click=move |_| {
                                    let new_theme = if theme.get() == "dark" { "light" } else { "dark" };
                                    set_theme.set(new_theme.to_string());
                                    save_config_action.dispatch(()); // Persist choice
                                }>
                                    <span class="toggle-label">"Theme Mode"</span>
                                    <button class="theme-toggle-btn">
                                        {move || if theme.get() == "dark" { "🌙 Dark" } else { "☀️ Light" }}
                                    </button>
                                </div>
                                <h3>"Chat Settings"</h3>
                                <h3>"Custom Tab Config"</h3>
                                <div class="filter-grid">
                                    {vec!["WORLD", "GUILD", "PARTY", "LOCAL"].into_iter().map(|channel| {
                                        let ch = channel.to_string();
                                        let ch_clone = ch.clone();
                                        view! {
                                            <label class="checkbox-row">
                                                <input type="checkbox"
                                                    checked=move || custom_filters.get().contains(&ch_clone)
                                                    on:change=move |ev| {
                                                        let checked = event_target_checked(&ev);
                                                        set_custom_filters.update(|f| {
                                                            if checked { f.push(ch.clone()); }
                                                            else { f.retain(|x| x != &ch); }
                                                        });
                                                        save_config_action.dispatch(()); // Auto-save
                                                    }
                                                />
                                                <span>{channel}</span>
                                            </label>
                                        }
                                    }).collect_view()}
                                </div>
                                <div class="setting-row">
                                    <span>"Message Limit"</span>
                                    <input type="number"
                                        prop:value=move || chat_limit.get()
                                        on:input=move |ev| {
                                            let val = event_target_value(&ev).parse::<usize>().unwrap_or(1000);
                                            set_chat_limit.set(val);
                                            save_config_action.dispatch(()); // Auto-save
                                        }
                                        class="limit-input"
                                    />
                                </div>
                                <h3>"Tab Visibility"</h3>
                                <div class="toggle-row">
                                    <span class="toggle-label">"Show System Tab"</span>
                                    <input type="checkbox"
                                        prop:checked=move || show_system_tab.get()
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            set_show_system_tab.set(checked);
                                            save_config_action.dispatch(());
                                        }
                                    />
                                </div>
                                <h3>"Log Detail"</h3>
                                <div class="toggle-row">
                                    <span class="toggle-label">"Enable Debug Logs (Technical)"</span>
                                    <input type="checkbox"
                                        prop:checked=move || is_debug.get()
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            set_is_debug.set(checked);
                                            save_config_action.dispatch(());
                                        }
                                    />
                                </div>
                                <h3>"About"</h3>
                                <p>"Blue Protocol Chat Translator v1.0"</p>
                                <a href="https://github.com/enjay27/bpsr-translator" target="_blank" class="github-link">
                                    <svg viewBox="0 0 16 16" width="20" height="20" fill="currentColor">
                                        <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path>
                                    </svg>
                                    " GitHub Repository"
                                </a>
                            </div>
                        </div>
                    </div>
                </div>
            </Show>

            <style>
                "
                /* --- 1. GLOBAL RESET & SCROLLBAR --- */
                html, body {
                    margin: 0;
                    padding: 0;
                    width: 100%;
                    height: 100%;
                    overflow: hidden; /* Prevent outer window scroll */
                    background: #121212;
                    font-family: 'Segoe UI', sans-serif;
                    color: #fff;
                }

                /* Custom Slim Scrollbar */
                .chat-container::-webkit-scrollbar { width: 6px; }
                .chat-container::-webkit-scrollbar-track { background: transparent; }
                .chat-container::-webkit-scrollbar-thumb {
                    background: #444;
                    border-radius: 3px;
                    transition: background 0.2s;
                }
                .chat-container::-webkit-scrollbar-thumb:hover { background: #00ff88; }

                /* --- 2. MAIN LAYOUT --- */
                .chat-app {
                    display: flex;
                    flex-direction: column;
                    height: 100vh;
                    background: rgba(var(--bg-rgb), var(--overlay-opacity)) !important;
                    color: var(--text-main);
                    transition: all 0.3s ease; /* Smooth transition for Compact Mode */
                }

                /* --- 3. TAB BAR & NAVIGATION --- */
                .tab-bar {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    background: rgba(var(--bg-rgb), var(--overlay-opacity));
                    border-bottom: 1px solid var(--border);
                    height: 42px; /* Fixed height for consistency */
                    transition: height 0.3s ease;
                }

                .tabs { display: flex; flex: 1; height: 100%; }

                .tab-btn {
                    flex: 1;
                    border: none;
                    background: none;
                    cursor: pointer;
                    font-weight: bold;
                    font-size: 0.9rem;
                    opacity: 0.6;
                    border-bottom: 2px solid transparent;
                    transition: all 0.2s;
                    color: #888;
                }
                .tab-btn:hover, .tab-btn.active { opacity: 1; background: #252525; }

                /* Tab Colors */
                .tab-btn[data-tab='전체'] { color: var(--text-main); }
                .tab-btn.active[data-tab='전체'] { border-bottom-color: var(--text-main); }

                .tab-btn[data-tab='월드'] { color: var(--world-color); }
                .tab-btn.active[data-tab='월드'] { border-bottom-color: var(--world-color); }

                .tab-btn[data-tab='길드'] { color: var(--guild-color); }
                .tab-btn.active[data-tab='길드'] { border-bottom-color: var(--guild-color); }

                .tab-btn[data-tab='파티'] { color: #4FC3F7; }
                .tab-btn.active[data-tab='파티'] { border-bottom-color: #4FC3F7; }

                .tab-btn[data-tab='로컬'] { color: #BDBDBD; }
                .tab-btn.active[data-tab='로컬'] { border-bottom-color: #BDBDBD; }

                .tab-btn[data-tab='시스템'] { color: #FFD54F; }
                .tab-btn.active[data-tab='시스템'] { border-bottom-color: #FFD54F; }

                /* Control Area (Buttons) */
                .control-area {
                    padding: 0 10px;
                    display: flex;
                    align-items: center;
                    gap: 8px;
                }

                .icon-btn {
                    position: relative;
                    background: transparent;
                    border: none;
                    cursor: pointer;
                    font-size: 1.1rem;
                    padding: 6px;
                    border-radius: 4px;
                    color: #888;
                    transition: all 0.2s;
                    display: flex; align-items: center; justify-content: center;
                }
                .restart-badge {
                    position: absolute;
                    top: -2px;      /* Positioned slightly outside the icon */
                    right: -2px;
                    width: 10px;
                    height: 10px;
                    background: #ffd740; /* Yellow for 'Warning/Restart Pending' */
                    border: 2px solid var(--bg-panel); /* Creates a clean gap */
                    border-radius: 50%;
                    box-shadow: 0 0 5px rgba(255, 215, 64, 0.5);
                    z-index: 20;    /* Ensure it sits above the icon */
                    pointer-events: none;
                }
                .icon-btn:hover { background: rgba(255,255,255,0.1); color: #fff; }
                .icon-btn.danger:hover { color: #ff4444; background: rgba(255,68,68,0.1); }

                .sync-btn {
                    position: relative;        /* [CRITICAL] Anchor for the absolute dot */
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    padding: 4px 8px;
                    gap: 4px;
                    min-width: 40px;
                    background: #2a2a2a;
                    border: 1px solid #444;
                    border-radius: 4px;
                    cursor: pointer;
                    transition: all 0.2s;
                }
                [data-theme='light'] .sync-btn {
                    background: #fcfcfc;
                    border-color: #ffffff;
                }
                .sync-btn:hover { border-color: #00ff88; color: #00ff88; }
                .update-dot {
                    position: absolute;
                    /* Positions dot on the top-right edge */
                    top: -5px;
                    right: -5px;

                    width: 10px;
                    height: 10px;
                    background: #ff4444;       /* High-visibility red */
                    border: 2px solid #1e1e1e; /* Creates a clean gap from the button edge */
                    border-radius: 50%;
                    box-shadow: 0 0 5px rgba(255, 68, 68, 0.6);
                    z-index: 10;
                    pointer-events: none;      /* Prevents the dot from blocking button clicks */
                }
                [data-theme='light'] .update-dot {
                    border-color: #ffffff;
                }

                /* --- 4. CHAT CONTAINER --- */
                .chat-container {
                    flex: 1;
                    overflow-y: auto;
                    padding: 10px;
                    position: relative; /* For sticky elements */
                }

                /* Chat Rows */
                .chat-row {
                    position: relative;
                    margin-bottom: 8px;
                    padding: 6px 10px;
                    border-radius: 4px;
                    border-left: 3px solid transparent;
                    color: #ddd;
                    overflow: visible !important; /* Allow context menu to pop out */
                }

                /* Channel Specific Styles */
                .chat-row[data-channel='LOCAL'] { border-left-color: #BDBDBD; background: rgba(189,189,189,0.05); }
                .chat-row[data-channel='LOCAL'] .nickname { color: #BDBDBD; }

                .chat-row[data-channel='PARTY'] { border-left-color: #4FC3F7; background: rgba(79,195,247,0.08); }
                .chat-row[data-channel='PARTY'] .nickname { color: #4FC3F7; }

                .chat-row[data-channel='GUILD'] { border-left-color: var(--guild-color); background: var(--bg-panel); }
                .chat-row[data-channel='GUILD'] .nickname { color: var(--guild-color); }

                .chat-row[data-channel='WORLD'] { border-left-color: var(--world-color); background: var(--bg-panel); }
                .chat-row[data-channel='WORLD'] .nickname { color: var(--world-color); }

                .chat-row[data-channel='SYSTEM'] { border-left-color: #FFD54F; background: rgba(255,213,79,0.08); }
                .chat-row[data-channel='SYSTEM'] .nickname { color: #FFD54F; }

                .level-badge {
                    font-size: 0.75rem;      /* Increased from 0.65rem for balance */
                    font-weight: 900;
                    padding: 2px 8px;        /* Slightly wider padding */
                    border-radius: 4px;
                    margin-right: 10px;
                    color: #000;
                    vertical-align: middle;
                    display: inline-flex;
                    align-items: center;
                }

                /* --- 1. SHARED COMPONENT STYLES --- */
                .clickable {
                    cursor: pointer;
                    transition: all 0.2s ease;
                }
                .clickable:hover {
                    filter: brightness(1.2);
                    text-decoration: underline;
                }

                /* Base System Log Row */
                .chat-row.system-log {
                    border-left: 4px solid transparent;
                    margin-bottom: 4px;
                    background: rgba(var(--bg-rgb), 0.3);
                    font-family: 'Consolas', 'Monaco', monospace; /* Technical feel */
                }

                .chat-row.system-log .msg-header {
                    display: flex;
                    align-items: center;
                    margin-bottom: 6px;
                }

                .level-badge {
                    font-size: 0.75rem;       /* Scaled for visual balance with larger source */
                    font-weight: 900;
                    padding: 2px 8px;
                    border-radius: 4px;
                    margin-right: 10px;
                }

                .source-tag {
                    font-size: 0.95rem;      /* Matched to player nicknames */
                    font-weight: 800;        /* Matched to player nicknames */
                    margin-right: 12px;
                    text-transform: uppercase;
                    letter-spacing: 0.5px;
                }

                /* --- 2. LEVEL-SPECIFIC COLORING (DARK MODE) --- */
                /* Consolidating logic into attribute-based rules */

                .chat-row[data-level='error']   { border-left-color: #ff5252; color: #ff8a80; }
                .chat-row[data-level='warn']    { border-left-color: #ffd740; color: #ffe57f; }
                .chat-row[data-level='success'] { border-left-color: #69f0ae; color: #b9f6ca; }
                .chat-row[data-level='info']    { border-left-color: #40c4ff; color: #81d4fa; }
                .chat-row[data-level='debug']   { border-left-color: #757575; color: #9e9e9e; opacity: 0.7; }

                /* Apply badge backgrounds */
                .chat-row[data-level='error'] .level-badge   { background: #ff5252; }
                .chat-row[data-level='warn'] .level-badge    { background: #ffd740; }
                .chat-row[data-level='success'] .level-badge { background: #69f0ae; }
                .chat-row[data-level='info'] .level-badge    { background: #40c4ff; }
                .chat-row[data-level='debug'] .level-badge   { background: #757575; color: #fff; }

                /* --- 3. TOASTS & OVERLAYS --- */
                .system-filter-toast,
                .scroll-lock-toast-bottom {
                    position: sticky;
                    left: 0;
                    right: 0;
                    margin: 0 auto; /* [FIX] Force stable centering regardless of parent width */
                    width: max-content;
                    z-index: 110;
                    display: flex;
                    align-items: center;
                    gap: 12px;
                    border-radius: 20px;
                    box-shadow: 0 4px 15px rgba(0,0,0,0.4);
                    animation: fadeIn 0.2s ease-out;
                }

                .system-filter-toast {
                    top: 10px;
                    background: #333;
                    color: #fff;
                    padding: 6px 16px;
                    border: 1px solid #555;
                    font-size: 0.85rem;
                }

                .scroll-lock-toast-bottom {
                    bottom: 10px;
                    background: var(--system-color);
                    color: #000;
                    padding: 8px 20px;
                    font-weight: 800;
                    font-size: 0.85rem;
                    cursor: pointer;
                }

                .filter-reset-btn {
                    background: rgba(255,255,255,0.1);
                    border: none;
                    color: #ff5252;
                    cursor: pointer;
                    width: 22px;
                    height: 22px;
                    border-radius: 50%;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    font-weight: bold;
                }

                /* --- 4. LIGHT MODE OVERRIDES --- */
                [data-theme='light'] .chat-row.system-log {
                    background: rgba(0, 0, 0, 0.02);
                    border-bottom: 1px solid #eee;
                }

                [data-theme='light'] .source-tag {
                    color: #111111;
                }

                [data-theme='light'] .chat-row[data-level='error']   { border-left-color: #c62828; color: #b71c1c; }
                [data-theme='light'] .chat-row[data-level='warn']    { border-left-color: #f57f17; color: #e65100; }
                [data-theme='light'] .chat-row[data-level='success'] { border-left-color: #2e7d32; color: #1b5e20; }
                [data-theme='light'] .chat-row[data-level='info']    { border-left-color: #0288d1; color: #01579b; }
                [data-theme='light'] .chat-row[data-level='debug']   { border-left-color: #616161; color: #424242; }

                [data-theme='light'] .level-badge { color: #fff; }

                [data-theme='light'] .system-filter-toast {
                    background: #ffffff;
                    color: #111111;
                    border: 1px solid #dee2e6;
                    box-shadow: 0 4px 12px rgba(0,0,0,0.1);
                }

                [data-theme='light'] .scroll-lock-toast-bottom {
                    background: #fff9c4;
                    border: 1px solid #fbc02d;
                    color: #333;
                }

                /* --- 5. ANIMATIONS --- */
                @keyframes slideDown { from { opacity: 0; transform: translate(-50%, -10px); } }
                @keyframes slideUp   { from { opacity: 0; transform: translate(-50%, 10px); } }

                /* --- 5. MESSAGE CONTENT --- */
                .msg-header {
                    display: flex; align-items: baseline; gap: 8px;
                    margin-bottom: 4px; opacity: 0.9; position: relative;
                }

                .nickname {
                    font-size: 0.95rem; font-weight: 800; cursor: pointer;
                    transition: all 0.2s; border-bottom: 1px dashed transparent;
                }
                .nickname:hover { filter: brightness(1.2); text-decoration: underline; }
                .nickname.active { color: #00ff88 !important; border-bottom: 1px solid #00ff88; }

                .lvl { font-size: 0.75rem; color: #666; }
                .time { font-size: 0.75rem; color: #555; margin-left: auto; }

                .lvl, .time {
                    color: var(--text-muted); /* Ensures meta-info isn't too faint */
                }

                .msg-wrapper { display: flex; align-items: flex-end; gap: 8px; }

                .msg-body {
                    position: relative; width: fit-content; max-width: 85%;
                    background: var(--bg-bubble);
                    padding: 8px 12px;
                    border-radius: 0 12px 12px 12px;
                    margin-top: 2px;
                    box-shadow: 0 2px 4px rgba(0,0,0,0.2);
                    color: var(--text-main);
                    border: 1px solid var(--border);
                }

                .original {
                    font-size: 0.9rem;
                    line-height: 1.4;
                    font-weight: 500;
                    color: var(--text-main);
                }
                .translated {
                    color: var(--accent); /* Now uses the deep accent defined in :root/light */
                    font-weight: 700;
                    margin-top: 4px;
                    font-size: 0.95rem;
                }

                .copy-btn {
                    background: transparent; border: none; color: #555;
                    cursor: pointer; opacity: 0; transition: all 0.2s; padding: 4px;
                }
                .msg-wrapper:hover .copy-btn { opacity: 1; }
                .copy-btn:hover { color: #00ff88; transform: scale(1.1); }

                /* --- 6. CONTEXT MENU & OVERLAY --- */
                .context-menu {
                    position: absolute; top: calc(100% + 4px); left: 0;
                    background: #1e1e1e !important;
                    border: 1px solid #00ff88; border-radius: 6px;
                    padding: 4px; display: flex; flex-direction: column; gap: 2px;
                    z-index: 10002; /* Topmost */
                    box-shadow: 0 8px 20px rgba(0,0,0,0.8);
                    min-width: 140px; pointer-events: auto;
                    animation: fadeIn 0.1s ease-out;
                }

                .menu-item {
                    background: transparent; border: none; color: #eee;
                    padding: 8px 12px; text-align: left; cursor: pointer;
                    font-size: 0.85rem; border-radius: 4px;
                    display: flex; align-items: center; gap: 8px;
                    pointer-events: auto;
                }
                .menu-item:hover { background: #00ff88; color: #000; }

                .menu-overlay {
                    position: fixed; top: 0; left: 0; width: 100vw; height: 100vh;
                    z-index: 10000; /* Below menu, above chat */
                    background: rgba(0,0,0,0.2);
                }

                /* --- 7. TOASTS (FILTER & NEW MESSAGE) --- */
                .filter-overlay-container {
                    position: sticky; top: 10px; left: 0; right: 0;
                    display: flex; justify-content: center;
                    pointer-events: none; z-index: 60;
                }

                .filter-chip, .new-msg-toast {
                    pointer-events: auto;
                    padding: 6px 12px; border-radius: 20px;
                    font-weight: 700; font-size: 0.85rem;
                    box-shadow: 0 4px 12px rgba(0,0,0,0.5);
                    display: flex; align-items: center; gap: 8px;
                    color: #000; /* Default text color for bright backgrounds */
                    transition: background 0.3s;
                }

                .new-msg-toast {
                    position: sticky; top: 10px; left: 100%; margin-right: 15px;
                    width: max-content; z-index: 50; cursor: pointer;
                    animation: slideInRight 0.3s ease-out;
                }
                .new-msg-toast:hover { transform: scale(1.05); }

                .filter-close-btn {
                    background: rgba(0,0,0,0.1); border: none; width: 20px; height: 20px;
                    border-radius: 50%; display: flex; align-items: center; justify-content: center;
                    cursor: pointer; font-size: 0.7rem; color: #000;
                }
                .filter-close-btn:hover { background: rgba(0,0,0,0.3); }

                /* Dynamic Color Logic for Toasts */
                .filter-chip[data-filter-type='전체'], .new-msg-toast[data-filter-type='전체'] { background: rgba(255,255,255,0.95); }
                .filter-chip[data-filter-type='월드'], .new-msg-toast[data-filter-type='월드'] { background: rgba(186,104,200,0.95); }
                .filter-chip[data-filter-type='길드'], .new-msg-toast[data-filter-type='길드'] { background: rgba(129,199,132,0.95); }
                .filter-chip[data-filter-type='파티'], .new-msg-toast[data-filter-type='파티'] { background: rgba(79,195,247,0.95); }
                .filter-chip[data-filter-type='로컬'], .new-msg-toast[data-filter-type='로컬'] { background: rgba(189,189,189,0.95); }
                .filter-chip[data-filter-type='시스템'], .new-msg-toast[data-filter-type='시스템'] { background: rgba(255,213,79,0.95); }

                /* --- 8. COMPACT MODE OVERRIDES --- */
                .chat-app.compact .tab-bar {
                    height: 32px; padding: 0 8px; background: #151515;
                }

                .compact-tab-indicator {
                    display: flex; align-items: center; gap: 8px;
                    font-size: 0.9rem; font-weight: bold; color: #fff;
                }
                .indicator-dot {
                    width: 8px; height: 8px; border-radius: 50%; background: #fff;
                }
                .indicator-dot[data-tab='월드'] { background: #BA68C8; }
                .indicator-dot[data-tab='길드'] { background: #81C784; }
                .indicator-dot[data-tab='파티'] { background: #4FC3F7; }
                /* ... (Add other colors if needed) ... */

                .chat-app.compact .chat-container { padding: 4px; }
                .chat-app.compact .chat-row { margin-bottom: 2px; padding: 2px 4px; }
                .chat-app.compact .msg-header { margin-bottom: 0; font-size: 0.8rem; }
                .chat-app.compact .msg-body {
                    border: 1px solid var(--border);
                    padding: 4px 8px;
                    margin-top: 1px;
                    border-radius: 4px;
                }

                /* HIDE ORIGINAL TEXT ONLY IF TRANSLATION EXISTS */
                .chat-app.compact .msg-body.has-translation .original { display: none; }

                .chat-app.compact .translated { margin-top: 0; font-size: 0.9rem; }
                .chat-app.compact .trans-tag { display: none; }

                /* --- ANIMATIONS --- */
                @keyframes fadeIn { from { opacity: 0; transform: translateY(-5px); } to { opacity: 1; transform: translateY(0); } }
                @keyframes slideInRight { from { opacity: 0; transform: translateX(20px); } to { opacity: 1; transform: translateX(0); } }

                /* --- RESPONSIVE TAB LABELS --- */
                .tab-full { display: inline; }
                .tab-short { display: none; }

                /* Switch to Emojis on narrow screens */
                @media (max-width: 600px) {
                    .tab-full { display: none; }

                    .tab-short {
                        display: inline-block;
                        font-size: 1.2rem; /* Emojis need to be slightly larger */
                        line-height: 1;
                        filter: drop-shadow(0 0 1px rgba(0,0,0,0.5)); /* enhance visibility */
                    }

                    /* Adjust button spacing for icons */
                    .tab-btn {
                        padding: 8px 6px;
                        min-width: 35px;
                    }

                    .control-area { gap: 4px; padding-right: 5px; }
                }

                /* --- COLOR VERIFICATION --- */
                /* These rules ensure that even if the Emoji is colored by default,
                   the Active Underline clearly indicates the channel color.
                */
                .tab-btn.active {
                    border-bottom-width: 3px; /* Make the color bar thicker for visibility */
                    background: rgba(255, 255, 255, 0.05); /* Slight highlight */
                }

                /* --- PIN BUTTON STATE --- */
                .icon-btn.active-pin {
                    color: #00ff88; /* Green Icon */
                    background: rgba(0, 255, 136, 0.1); /* Subtle Green Background */
                    box-shadow: 0 0 8px rgba(0, 255, 136, 0.2); /* Glow Effect */
                    border: 1px solid rgba(0, 255, 136, 0.3);
                }

                .icon-btn span {
                    transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1); /* Bouncy rotation */
                }

                /* --- SETTINGS MODAL --- */
                .settings-overlay {
                    position: fixed; top: 0; left: 0; right: 0; bottom: 0;
                    background: rgba(0, 0, 0, 0.7);
                    display: flex; justify-content: center; align-items: center;
                    z-index: 20000; /* Highest priority */
                    backdrop-filter: blur(2px);
                    animation: fadeIn 0.2s;
                }

                .settings-modal {
                    width: 90%; max-width: 400px;
                    max-height: 85vh;
                    display: flex;
                    flex-direction: column;
                    background: var(--bg-panel);
                    border: 1px solid var(--border);
                    border-radius: 8px;
                    box-shadow: 0 10px 30px rgba(0,0,0,0.3);
                    overflow: hidden;
                    animation: slideUp 0.3s cubic-bezier(0.16, 1, 0.3, 1);
                }

                .settings-content {
                    padding: 16px;
                    display: flex;
                    flex-direction: column;
                    gap: 20px;
                    overflow-y: auto; /* Enable vertical scrolling for content */
                    flex: 1; /* Allow content to fill available space in the modal */
                }

                .settings-content::-webkit-scrollbar { width: 6px; }
                .settings-content::-webkit-scrollbar-thumb {
                    background: #444;
                    border-radius: 3px;
                }
                .settings-content::-webkit-scrollbar-thumb:hover { background: var(--accent); }

                .settings-header {
                    display: flex; justify-content: space-between; align-items: center;
                    padding: 12px 16px;
                    background: var(--bg-bubble);
                    border-bottom: 1px solid var(--border);
                    color: var(--text-main);
                }
                .settings-header h2 { margin: 0; font-size: 1.1rem; color: var(--text-main); }

                .close-btn {
                    background: none; border: none; color: #aaa;
                    font-size: 1.2rem; cursor: pointer; padding: 4px;
                }
                .close-btn:hover { color: #fff; }

                .settings-content {
                    padding: 16px;
                    overflow-y: auto;
                    display: flex;
                    flex-direction: column;
                    gap: 20px;
                    color: var(--text-main);
                }

                .setting-group h3 {
                    margin: 0 0 10px 0;
                    font-size: 0.9rem;
                    color: var(--text-muted);
                    text-transform: uppercase;
                    letter-spacing: 0.5px;
                    border-bottom: 1px solid var(--border);
                    padding-bottom: 4px;
                }

                .filter-grid {
                    display: grid;
                    grid-template-columns: 1fr 1fr;
                    gap: 10px;
                    padding: 10px 0;
                }

                .checkbox-row {
                    display: flex;
                    align-items: center;
                    gap: 10px;
                    cursor: pointer;
                    font-size: 0.9rem;
                    padding: 5px;
                    border-radius: 4px;
                }

                .checkbox-row:hover { background: rgba(255, 255, 255, 0.05); }

                .checkbox-row input[type="checkbox"] {
                    accent-color: #00ff88;
                    width: 16px;
                    height: 16px;
                }

                /* Custom Tab highlight */
                .tab-btn[data-tab='커스텀'] { color: #00ff88; }
                .tab-btn.active[data-tab='커스텀'] { border-bottom-color: #00ff88; }

                .setting-row {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    padding: 8px 0;
                }

                .limit-input {
                    width: 80px; /* Constrain the width */
                    background: var(--bg-bubble);
                    border: 1px solid #444;
                    padding: 4px 8px;
                    border_radius: 4px;
                    text-align: right;
                    font-family: 'Consolas', monospace;
                }

                .limit-input:focus {
                    border-color: #00ff88;
                    outline: none;
                }

                .limit-input, .tier-select, .checkbox-row {
                    color: var(--text-main);
                }

                .tier-extreme {
                    color: #ff00ff; /* Neon Magenta */
                    font-weight: 600;
                    text-shadow: 0 0 8px rgba(255, 0, 255, 0.4);
                }

                /* --- GITHUB LINK BUTTON --- */
                .github-link {
                    display: flex; align-items: center; gap: 10px;
                    background: #333; color: #fff;
                    text-decoration: none;
                    padding: 10px; border-radius: 6px;
                    font-weight: bold; font-size: 0.95rem;
                    transition: all 0.2s;
                    border: 1px solid transparent;
                }
                .github-link:hover {
                    background: #444;
                    border-color: #aaa;
                    transform: translateY(-2px);
                }

                /* --- TOGGLES & BUTTONS --- */
                .toggle-row {
                    display: flex; justify-content: space-between; align-items: center;
                    padding: 8px 0; cursor: pointer;
                    font-size: 0.95rem;
                    var(--text-main);
                }
                .toggle-row:hover { color: #fff; }
                .toggle-label {
                    font-size: 0.9rem;
                    color: var(--text-main);
                }

                .action-btn {
                    width: 100%; padding: 10px;
                    background: var(--bg-bubble);
                    border: 1px solid var(--border);
                    color: var(--text-main);
                    border-radius: 4px; cursor: pointer;
                    transition: all 0.2s; text-align: left;
                }
                .action-btn:hover { background: #333; border-color: #00ff88; color: #00ff88; }

                @keyframes slideUp {
                    from { opacity: 0; transform: translateY(20px); }
                    to { opacity: 1; transform: translateY(0); }
                }

                /* Slider Styles */
                .slider-container {
                    display: flex;
                    align-items: center;
                    gap: 12px;
                }

                input[type='range'] {
                    accent-color: var(--accent);
                    cursor: pointer;
                    width: 120px;
                }

                .hint {
                    font-size: 0.75rem;
                }

                .opacity-value {
                    font-family: 'Consolas', monospace;
                    font-size: 0.85rem;
                    min-width: 40px;
                    color: var(--accent);
                }

                /* --- Light / Dark Theme --- */
                :root {
                    /* --- DARK MODE (Default) --- */
                    --bg-main: #121212;
                    --bg-panel: #1e1e1e;
                    --bg-bubble: #252525;
                    --text-main: #eeeeee;
                    --text-muted: #888888;
                    --border: #333333;
                    --accent: #00ff88;
                    --bg-rgb: 18, 18, 18;

                    /* Channel Colors (Pastel for Dark) */
                    --world-color: #BA68C8;
                    --guild-color: #81C784;
                    --party-color: #4FC3F7;
                    --local-color: #BDBDBD;
                    --system-color: #FFD54F;

                    --overlay-opacity: 0.85; /* Default */
                }

                [data-theme='light'] {
                    /* --- LIGHT MODE (High Contrast) --- */
                    --bg-main: #fcfcfc;    /* Clean page background */
                    --bg-panel: #ffffff;   /* Solid white for navigation */
                    --bg-bubble: #e9ecef;  /* Light gray bubble for visibility */
                    --bg-rgb: 252, 252, 252;

                    --text-main: #111111;  /* "Ink" black for original Japanese */
                    --text-muted: #495057; /* Dark gray for secondary info */

                    --border: #dee2e6;     /* Subtle divider lines */
                    --accent: #006b3d;     /* Deep Forest Green for translations */

                    /* Channel Colors (Deepened for readability on white) */
                    --world-color: #7b1fa2;
                    --guild-color: #2e7d32;
                    --party-color: #0277bd;
                    --local-color: #616161;
                    --system-color: #f57f17;
                }
                [data-theme='light'] .chat-container::-webkit-scrollbar-thumb {
                    background: #bbb; /* Darker thumb for white background */
                }

                .custom-title-bar {
                    height: 30px;
                    background: var(--bg-panel); /* Adapts to theme */
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    padding-left: 10px;
                    user-select: none;
                    cursor: default;
                    position: relative;
                    border-bottom: 1px solid var(--border); /* Adapts to theme */
                }

                .drag-handle {
                    position: absolute;
                    top: 0;
                    left: 0;
                    width: 100%;
                    height: 100%;
                    z-index: 1; /* Sits below the buttons */
                    cursor: default;
                }

                .window-title {
                    position: relative;
                    z-index: 2; /* Ensures text is visible but doesn't block handle */
                    pointer-events: none; /* Mouse clicks pass through to the drag-handle */
                    font-size: 0.75rem;
                    color: var(--text-muted);
                    font-weight: 600;
                }

                .window-controls {
                    position: relative;
                    display: flex;
                    align-items: center;
                    height: 100%;
                    z-index: 10;
                    -webkit-app-region: no-drag;
                }

                .win-btn {
                    width: 45px;
                    height: 100%;
                    background: transparent;
                    border: none;
                    color: var(--text-main); /* Near-black in light mode */
                    cursor: pointer;
                    transition: background 0.2s;
                    position: relative;
                    z-index: 11;
                    pointer-events: auto;
                }

                .win-btn:hover { background: rgba(128, 128, 128, 0.2); }
                .win-btn.close:hover { background: #e81123; color: #fff; }

                /* Light Mode specific polish */
                [data-theme='light'] .custom-title-bar {
                    background: #f1f3f5; /* Slightly different gray for the bar in light mode */
                }

                /* Setup View */
                .setup-view {
                    display: flex;
                    flex-direction: column;
                    align-items: center;    /* Centers children horizontally */
                    justify-content: center; /* Centers children vertically */
                    height: 100vh;           /* Full viewport height */
                    width: 100%;
                    gap: 24px;               /* Consistent spacing between elements */
                    padding: 20px;
                    text-align: center;
                }

                .primary-btn {
                    padding: 14px 28px;
                    background: var(--accent);
                    color: #000;
                    border: none;
                    border-radius: 8px;
                    font-size: 1rem;
                    font-weight: 800;
                    cursor: pointer;
                    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
                    box-shadow: 0 4px 12px rgba(0, 255, 136, 0.3);
                }

                .primary-btn:hover {
                    transform: translateY(-2px);
                    box-shadow: 0 6px 20px rgba(0, 255, 136, 0.4);
                    filter: brightness(1.1);
                }

                .status-card {
                    background: var(--bg-bubble);
                    padding: 20px;
                    border-radius: 12px;
                    border: 1px solid var(--border);
                    text-align: center;
                    box-shadow: 0 4px 15px rgba(0,0,0,0.2);
                }

                .progress-bar {
                    width: 100%;
                    height: 12px;
                    background: rgba(0,0,0,0.3);
                    border-radius: 6px;
                    overflow: hidden;
                    margin-top: 15px;
                    position: relative; /* For label positioning */
                }

                .progress-bar .fill {
                    height: 100%;
                    background: linear-gradient(90deg, #00ff88, #00bfff); /* Visual gradient */
                    transition: width 0.4s cubic-bezier(0.1, 0.7, 0.1, 1); /* Smooth movement */
                    box-shadow: 0 0 10px rgba(0, 255, 136, 0.5);
                }

                /* UI Addition: Percentage Text */
                .progress-label {
                    margin-top: 8px;
                    font-size: 0.8rem;
                    color: var(--accent);
                    font-weight: bold;
                }

                /* Compact Container */
                .status-dot-container {
                    display: flex;
                    align-items: center;
                    gap: 5px;             /* Reduced from 8px */
                    padding: 0px 8px;     /* Reduced from 4px 10px */
                    background: rgba(0, 0, 0, 0.3);
                    border: 1px solid #444;
                    border-radius: 4px;   /* Squarer corners often look better in title bars */
                    font-size: 0.65rem;   /* Reduced from 0.75rem */
                    font-weight: 800;
                    color: #888;
                    height: 18px;         /* Fixed height to fit the 30px title bar perfectly */
                    transition: all 0.3s ease;
                }

                /* Compact Dot */
                .status-dot-container .pulse-dot {
                    width: 6px;           /* Reduced from 8px */
                    height: 6px;          /* Reduced from 8px */
                    background: #555;
                    border-radius: 50%;
                    transition: all 0.3s ease;
                    display: inline-block;
                }

                /* Online State */
                .status-dot-container.online {
                    color: #00ff88;
                    border-color: rgba(0, 255, 136, 0.3);
                    background: rgba(0, 255, 136, 0.05);
                }

                .status-dot-container.online .pulse-dot {
                    background: #00ff88;
                    box-shadow: 0 0 6px #00ff88; /* Tighter glow */
                    animation: pulse-animation-compact 2s infinite;
                }

                /* Adjusted Animation for smaller scale */
                @keyframes pulse-animation-compact {
                    0% {
                        transform: scale(0.95);
                        box-shadow: 0 0 0 0 rgba(0, 255, 136, 0.7);
                    }
                    70% {
                        transform: scale(1);
                        box-shadow: 0 0 0 4px rgba(0, 255, 136, 0); /* Reduced from 6px */
                    }
                    100% {
                        transform: scale(0.95);
                        box-shadow: 0 0 0 0 rgba(0, 255, 136, 0);
                    }
                }
                "
            </style>
        </main>
    }
}