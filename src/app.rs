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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct AppConfig {
    compact_mode: bool,
    always_on_top: bool,
    active_tab: String,
    chat_limit: usize,
    custom_tab_filters: Vec<String>,
    theme: String,
    overlay_opacity: f32,
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
    let (status_text, set_status_text) = signal("Initializing...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);

    let (active_tab, set_active_tab) = signal("전체".to_string());
    let (search_term, set_search_term) = signal("".to_string());

    let (name_cache, set_name_cache) = signal(std::collections::HashMap::<String, String>::new());

    // --- SEPARATE DATA STREAMS ---
    // 1. Game Chat (IndexMap for updates)
    let (chat_log, set_chat_log) = signal(IndexMap::<u64, RwSignal<ChatPacket>>::new());
    // 2. System Logs (VecDeque logic in frontend)
    let (system_log, set_system_log) = signal(Vec::<RwSignal<ChatPacket>>::new());

    let (dict_update_available, set_dict_update_available) = signal(false);

    let (compact_mode, set_compact_mode) = signal(false);
    let (is_pinned, set_is_pinned) = signal(false);

    let (show_settings, set_show_settings) = signal(false);
    let (chat_limit, set_chat_limit) = signal(1000);
    let (custom_filters, set_custom_filters) = signal(vec!["WORLD".to_string(), "GUILD".to_string(), "PARTY".to_string(), "LOCAL".to_string()]);

    let (theme, set_theme) = signal("dark".to_string());
    let (opacity, set_opacity) = signal(0.85f32);
    let (is_debug, set_is_debug) = signal(false);
    let (tier, set_tier) = signal("middle".to_string());

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

    // --- HELPERS ---
    let format_time = |ts: u64| {
        let date = js_sys::Date::new(&JsValue::from_f64(ts as f64 * 1000.0));
        format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
    };

    let is_japanese = |text: &str| {
        let re = js_sys::RegExp::new("[\\u3040-\\u309F\\u30A0-\\u30FF\\u4E00-\\u9FAF]", "");
        re.test(text)
    };

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

    // --- OPTIMIZED VIEW LOGIC ---
    let filtered_messages = Memo::new(move |_| {
        let tab = active_tab.get();
        let search = search_term.get().to_lowercase();
        let filters = custom_filters.get();

        let list_by_tab = match tab.as_str() {
            "시스템" => system_log.get(),
            "전체" => chat_log.get().values().cloned().collect(),
            "커스텀" => { // NEW: Custom filtering logic
                chat_log.get().values()
                    .filter(|m| filters.contains(&m.get().channel))
                    .cloned()
                    .collect::<Vec<_>>()
            },
            _ => {
                let key = match tab.as_str() {
                    "로컬" => "LOCAL", "파티" => "PARTY", "길드" => "GUILD", _ => "WORLD"
                };
                chat_log.get().values()
                    .filter(|m| m.get().channel == key)
                    .cloned()
                    .collect::<Vec<_>>()
            }
        };

        // Step B: Filter by Search Term (if exists)
        if search.is_empty() {
            list_by_tab
        } else {
            list_by_tab.into_iter().filter(|sig| {
                let msg = sig.get();
                // Check Nickname OR Message content
                msg.nickname.to_lowercase().contains(&search) ||
                    msg.message.to_lowercase().contains(&search)
            }).collect()
        }
    });

    // 1. STATE: Track if the user is currently at the bottom
    let (is_at_bottom, set_is_at_bottom) = signal(true);
    let (unread_count, set_unread_count) = signal(0); // [NEW] Tracks missed messages

    let (active_menu_id, set_active_menu_id) = signal(None::<u64>); // [NEW] Track open menu

    let chat_container_ref = create_node_ref::<html::Div>();

    // 2. EFFECT: Auto-scroll when messages update
    Effect::new(move |_| {
        // We track 'filtered_messages' so this runs ONLY when the visible list changes
        filtered_messages.track();

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
                    if let Ok(packet) = serde_json::from_value::<ChatPacket>(ev["payload"].clone()) {
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
                Ok(_) => { set_downloading.set(false); set_model_ready.set(true); }
                Err(_) => { set_downloading.set(false); }
            }
        });
    };

    // This gathers the current state of all signals and sends them to Rust.
    let save_config_action = Action::new_local(move |_: &()| {
        // Use get_untracked() to read values without creating subscriptions
        let config = AppConfig {
            compact_mode: compact_mode.get_untracked(),
            always_on_top: is_pinned.get_untracked(),
            active_tab: active_tab.get_untracked(),
            chat_limit: chat_limit.get_untracked(),
            custom_tab_filters: custom_filters.get_untracked(),
            theme: theme.get_untracked(),
            overlay_opacity: opacity.get_untracked(),
            is_debug: is_debug.get_untracked(),
            tier: tier.get_untracked(),
        };

        async move {
            // Send to Backend
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "config": config
            })).unwrap();
            let _ = invoke("save_config", args).await;
        }
    });

    // --- STARTUP HYDRATION ---
    Effect::new(move |_| {
        spawn_local(async move {

            // Load User Config
            if let Ok(res) = invoke("load_config", JsValue::NULL).await {
                if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(res) {
                    set_compact_mode.set(config.compact_mode);
                    set_active_tab.set(config.active_tab);
                    set_is_pinned.set(config.always_on_top);
                    set_chat_limit.set(config.chat_limit);
                    set_custom_filters.set(config.custom_tab_filters);
                    set_theme.set(config.theme);
                    set_opacity.set(config.overlay_opacity);
                    set_is_debug.set(config.is_debug);
                    set_tier.set(config.tier);

                    // Apply Window State (Backend)
                    if config.always_on_top {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                            "onTop": true
                        })).unwrap();
                        let _ = invoke("set_always_on_top", args).await;
                    }
                }
            }

            if let Ok(res) = invoke("check_dict_update", JsValue::NULL).await {
                if let Some(needed) = res.as_bool() {
                    set_dict_update_available.set(needed);
                }
            }

            if let Ok(res) = invoke("check_model_status", JsValue::NULL).await {
                if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(res) {
                    if status.exists {
                        setup_listeners();

                        // Hydrate GAME History
                        if let Ok(res) = invoke("get_chat_history", JsValue::NULL).await {
                            if let Ok(vec) = serde_wasm_bindgen::from_value::<Vec<ChatPacket>>(res) {
                                let sanitized_vec: Vec<(u64, RwSignal<ChatPacket>)> = vec.into_iter().map(|mut p| {
                                    if p.message.starts_with("emojiPic=") {
                                        p.message = "스티커 전송".to_string();
                                    } else if p.message.contains("<sprite=") {
                                        p.message = "이모지 전송".to_string();
                                    }
                                    (p.pid, RwSignal::new(p))
                                }).collect();

                                set_chat_log.set(sanitized_vec.into_iter().collect());
                            }
                        }

                        // Hydrate SYSTEM History
                        if let Ok(res) = invoke("get_system_history", JsValue::NULL).await {
                            if let Ok(vec) = serde_wasm_bindgen::from_value::<Vec<ChatPacket>>(res) {
                                set_system_log.set(vec.into_iter().map(|p| RwSignal::new(p)).collect());
                            }
                        }

                        let _ = invoke("start_sniffer_command", JsValue::NULL).await;
                        let _ = invoke("start_translator_sidecar", serde_wasm_bindgen::to_value(&serde_json::json!({"useGpu":true})).unwrap()).await;
                        set_model_ready.set(true);
                        set_status_text.set("Ready".to_string());
                    }
                }
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

            <Show when=move || model_ready.get() fallback=move || view! {
                <div class="setup-view">
                    <h1>"BPSR Translator"</h1>
                    <div class="status-card">
                        <p><strong>"Status: "</strong> {move || status_text.get()}</p>
                        <Show when=move || downloading.get()>
                            <div class="progress-bar">
                                <div class="fill" style:width=move || format!("{}%", progress.get())></div>
                            </div>
                        </Show>
                    </div>
                    <Show when=move || !model_ready.get() && !downloading.get()>
                        <button class="primary-btn" on:click=start_download>
                            "Install Translation Model (400MB)"
                        </button>
                    </Show>
                </div>
            }>
                <div class="custom-title-bar" data-tauri-drag-region>
                    <div class="window-title">"BPSR Translator"</div>
                    <div class="window-controls">
                        <button class="win-btn" on:click=move |_| { let _ = invoke("minimize_window", JsValue::NULL); }>"—"</button>
                        <button class="win-btn close" on:click=move |_| { let _ = invoke("close_window", JsValue::NULL); }>"✕"</button>
                    </div>
                </div>
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
                                if is_debug.get() {
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
                        <button class="icon-btn"
                            title="Settings & Info"
                            on:click=move |_| set_show_settings.set(true)
                        >
                            "⚙️"
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

                        set_is_at_bottom.set(at_bottom);

                        // If user manually scrolls to bottom, clear the unread count
                        if at_bottom {
                            set_unread_count.set(0);
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

                    <For each=move || filtered_messages.get()
                        key=|sig| sig.get_untracked().pid
                        children=move |sig| {
                            // This child closure now receives an individual RwSignal
                            let msg = sig.get();
                            let is_jp = is_japanese(&msg.message);
                            let is_system = msg.channel == "SYSTEM";
                            let pid = msg.pid;
                            let nick_for_copy = msg.nickname.clone();
                            let nick_for_filter = msg.nickname.clone();
                            let is_active = move || active_menu_id.get() == Some(pid);

                            view! {
                                <div class="chat-row"
                                     data-channel=move || sig.get().channel.clone()
                                     // [NEW] Lift the entire row when the menu is active
                                     style:z-index=move || if is_active() { "10001" } else { "1" }
                                >
                                    <div class="msg-header"
                                         style:position="relative"
                                         style:z-index=move || if is_active() { "1001" } else { "1" }
                                    >
                                        // [CHANGED] Added Click Handler
                                        <span class=move || if search_term.get() == sig.get().nickname { "nickname active" } else { "nickname" }
                                            on:click=move |ev| {
                                                // Stop the click from bubbling up (good practice)
                                                ev.stop_propagation();

                                                if active_menu_id.get() == Some(pid) {
                                                    set_active_menu_id.set(None);
                                                } else {
                                                    set_active_menu_id.set(Some(pid));
                                                }
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

                                        <Show when=is_active>
                                            <div class="context-menu" on:click=move |ev| ev.stop_propagation()>
                                                // OPTION 1: COPY
                                                <button class="menu-item" on:click={
                                                    // Create another clone for this specific closure
                                                    let n = nick_for_copy.clone();
                                                    move |_| {
                                                        copy_text(n.clone());
                                                        set_active_menu_id.set(None);
                                                    }
                                                }>
                                                    <span class="menu-icon">"📋"</span>
                                                    <span class="menu-text">"Copy Name"</span>
                                                </button>

                                                // OPTION 2: FILTER
                                                <button class="menu-item" on:click={
                                                    let n = nick_for_filter.clone();
                                                    move |_| {
                                                        if search_term.get_untracked() == n {
                                                            set_search_term.set("".into());
                                                        } else {
                                                            set_search_term.set(n.clone());
                                                        }
                                                        set_active_menu_id.set(None);
                                                    }
                                                }>
                                                    <span class="menu-icon">"🔍"</span>
                                                    <span class="menu-text">"Filter Chat"</span>
                                                </button>
                                            </div>
                                        </Show>

                                        <span class="lvl">"Lv." {move || sig.get().level}</span>
                                        <span class="time">{format_time(msg.timestamp)}</span>
                                    </div>
                                    <div class="msg-wrapper">

                                        // The Message Bubble
                                        <div class="msg-body"
                                             class:has-translation=move || sig.get().translated.is_some()
                                        >
                                            <div class="original">
                                                {if is_jp && !is_system { "[원문] " } else { "" }} {move || sig.get().message.clone()}
                                            </div>
                                            {move || sig.get().translated.clone().map(|text| view! {
                                                <div class="translated">"[번역] " {text}</div>
                                            })}
                                        </div>

                                        // The Copy Button (Now OUTSIDE the bubble)
                                        <button class="copy-btn" title="Copy Original"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                copy_text(sig.get().message.clone());
                                            }
                                        >
                                            "📋"
                                        </button>
                                    </div>
                                </div>
                            }
                        }/>
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
                                <h3>"Performance Tier"</h3>
                                <div class="tier-select">
                                    {vec!["low", "middle", "high"].into_iter().map(|t| {
                                        let t_val = t.to_string();
                                        let t_val_tier = t.to_string();
                                        view! {
                                            <label class="radio-row">
                                                <input type="radio" name="tier"
                                                    checked=move || tier.get() == t_val
                                                    on:change=move |_| {
                                                        set_tier.set(t_val_tier.clone());
                                                        save_config_action.dispatch(()); // Persist choice
                                                    }
                                                />
                                                <span>{t.to_uppercase()}</span>
                                            </label>
                                        }
                                    }).collect_view()}
                                </div>
                                <p class="hint">"High uses more VRAM but has better accuracy."</p>
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
                                    <span>"Theme Mode"</span>
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
                                <h3>"Tab Settings"</h3>
                                <div class="setting-row">
                                    <span>"Debug Mode"</span>
                                    <input type="checkbox"
                                        prop:checked=move || is_debug.get()
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            set_is_debug.set(checked);
                                            save_config_action.dispatch(()); // Persist change
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
                    font-size: 0.95rem; color: #ddd;
                }
                .toggle-row:hover { color: #fff; }

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
                    border-bottom: 1px solid var(--border); /* Adapts to theme */
                }

                .window-title {
                    font-size: 0.75rem;
                    color: var(--text-muted); /* Dark gray in light, light gray in dark */
                    font-weight: 600;
                }

                .window-controls {
                    display: flex;
                    height: 100%;
                }

                .win-btn {
                    width: 45px;
                    height: 100%;
                    background: transparent;
                    border: none;
                    color: var(--text-main); /* Near-black in light mode */
                    cursor: pointer;
                    transition: background 0.2s;
                }

                .win-btn:hover { background: rgba(128, 128, 128, 0.2); }
                .win-btn.close:hover { background: #e81123; color: #fff; }

                /* Light Mode specific polish */
                [data-theme='light'] .custom-title-bar {
                    background: #f1f3f5; /* Slightly different gray for the bar in light mode */
                }
                "
            </style>
        </main>
    }
}