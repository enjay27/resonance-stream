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

    // --- SEPARATE DATA STREAMS ---
    // 1. Game Chat (IndexMap for updates)
    let (chat_log, set_chat_log) = signal(IndexMap::<u64, RwSignal<ChatPacket>>::new());
    // 2. System Logs (VecDeque logic in frontend)
    let (system_log, set_system_log) = signal(Vec::<RwSignal<ChatPacket>>::new());

    let (dict_update_available, set_dict_update_available) = signal(false);

    let (compact_mode, set_compact_mode) = signal(false);

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
        let search = search_term.get().to_lowercase(); // Case-insensitive

        // Step A: Get the list based on Tab
        let list_by_tab = match tab.as_str() {
            "시스템" => system_log.get(),
            "전체" => chat_log.get().values().cloned().collect(),
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
            // LISTENER 1: Game Packets
            let packet_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    // [CHANGED] parsing to 'mut' so we can modify it
                    if let Ok(mut packet) = serde_json::from_value::<ChatPacket>(ev["payload"].clone()) {

                        // [NEW] Sticker Transformation Logic
                        // Detects "emojiPic=" pattern and replaces it with "스티커 전송"
                        if packet.message.starts_with("emojiPic=") {
                            packet.message = "스티커 전송".to_string();
                            // Optional: We can force translation to None since it's already "translated"
                            packet.translated = None;
                        }

                        let packet_clone = packet.clone();

                        set_chat_log.update(|log| {
                            if log.len() >= 1000 { log.shift_remove_index(0); }
                            log.insert(packet.pid, RwSignal::new(packet));
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

                        // Trigger Translation (Only if it's NOT a sticker anymore)
                        // Since "스티커 전송" is Korean, is_japanese() will return false, saving API calls.
                        if is_japanese(&packet_clone.message) {
                            spawn_local(async move {
                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                       "text": packet_clone.message, "pid": packet_clone.pid
                   })).unwrap();
                                let _ = invoke("manual_translate", args).await;
                            });
                        }
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

            let trans_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(resp) = serde_json::from_str::<serde_json::Value>(ev["payload"].as_str().unwrap_or("")) {
                        let target_pid = resp["pid"].as_u64().unwrap_or(0);
                        let translated_text = resp["translated"].as_str().unwrap_or_default().to_string();

                        // O(1) Lookup: Update ONLY the signal for this specific PID
                        chat_log.with_untracked(|log| {
                            if let Some(packet_sig) = log.get(&target_pid) {
                                packet_sig.update(|p| p.translated = Some(translated_text));
                                log!("Immediate Render Triggered for PID: {}", target_pid);
                            }
                        });
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("packet-event", &packet_closure).await;
            listen("system-event", &system_closure).await;
            listen("translator-event", &trans_closure).await;

            packet_closure.forget();
            system_closure.forget();
            trans_closure.forget();
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

    // --- STARTUP HYDRATION ---
    Effect::new(move |_| {
        spawn_local(async move {
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
                                set_chat_log.set(vec.into_iter().map(|p| (p.pid, RwSignal::new(p))).collect());
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
        <main class=move || if compact_mode.get() { "chat-app compact" } else { "chat-app" }>
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
                            {vec![
                                ("전체", "♾️"), // All (Infinity)
                                ("월드", "🌐"), // World (Globe)
                                ("길드", "🛡️"), // Guild (Shield)
                                ("파티", "⚔️"), // Party (Swords)
                                ("로컬", "📍"), // Local (Pin)
                                ("시스템", "⚙️") // System (Gear)
                            ].into_iter().map(|(full, short)| {
                                let t_full = full.to_string();
                                let t_click = t_full.clone();
                                let t_data = t_full.clone();
                                let t_tab = t_full.clone();

                                view! {
                                    <button
                                        class=move || if active_tab.get() == t_tab { "tab-btn active" } else { "tab-btn" }
                                        data-tab=t_data
                                        on:click=move |_| set_active_tab.set(t_click.clone())
                                        title=t_full
                                    >
                                        <span class="tab-full">{full}</span>
                                        <span class="tab-short">{short}</span>
                                    </button>
                                }
                            }).collect_view()}
                        </Show>
                    </div>

                    // --- DICTIONARY SYNC BUTTON ---
                    <div class="control-area">
                        <button class="icon-btn"
                            title=move || if compact_mode.get() { "Expand Mode" } else { "Compact Mode" }
                            on:click=move |_| set_compact_mode.update(|b| *b = !*b)
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

                        // 2. Sync Dictionary Button
                        <button class="sync-btn"
                            on:click=move |_| {
                                sync_dict_action.dispatch(());
                                set_dict_update_available.set(false);
                            }
                            disabled=is_syncing
                        >
                            {move || if is_syncing.get() { "동기화 중..." } else { "사전 업데이트" }}
                            <Show when=move || dict_update_available.get()>
                                <span class="update-dot"></span>
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

                    // 5. FLOATING BUTTON
                    // Show ONLY if there are unread messages (implies we are scrolled up)
                    <Show when=move || { unread_count.get() > 0 }>
                        <button class="scroll-bottom-btn"
                            on:click=move |_| {
                                if let Some(el) = chat_container_ref.get() {
                                    // 1. Scroll to bottom
                                    el.set_scroll_top(el.scroll_height());
                                    // 2. Reset states immediately
                                    set_is_at_bottom.set(true);
                                    set_unread_count.set(0);
                                }
                            }
                        >
                            // Dynamic Label: "⬇ 3 New Messages"
                            "⬇ " {move || unread_count.get()} " New Messages"
                        </button>
                    </Show>

                    <For each=move || filtered_messages.get()
                        key=|sig| sig.get_untracked().pid
                        children=move |sig| {
                            // This child closure now receives an individual RwSignal
                            let msg = sig.get();
                            let is_jp = is_japanese(&msg.message);
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
                                            {move || sig.get().nickname.clone()}
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
                                                {if is_jp { "[원문] " } else { "" }} {move || sig.get().message.clone()}
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
                    background: #121212;
                    transition: all 0.3s ease; /* Smooth transition for Compact Mode */
                }

                /* --- 3. TAB BAR & NAVIGATION --- */
                .tab-bar {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    background: #1e1e1e;
                    border-bottom: 1px solid #333;
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
                .tab-btn[data-tab='전체'] { color: #FFFFFF; }
                .tab-btn.active[data-tab='전체'] { border-bottom-color: #FFFFFF; }

                .tab-btn[data-tab='월드'] { color: #BA68C8; }
                .tab-btn.active[data-tab='월드'] { border-bottom-color: #BA68C8; }

                .tab-btn[data-tab='길드'] { color: #81C784; }
                .tab-btn.active[data-tab='길드'] { border-bottom-color: #81C784; }

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
                    background: #333; color: #aaa; border: 1px solid #444;
                    padding: 4px 10px; border-radius: 4px; font-size: 0.75rem;
                    cursor: pointer; transition: all 0.2s;
                }
                .sync-btn:hover { border-color: #00ff88; color: #00ff88; }

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

                .chat-row[data-channel='GUILD'] { border-left-color: #81C784; background: rgba(129,199,132,0.08); }
                .chat-row[data-channel='GUILD'] .nickname { color: #81C784; }

                .chat-row[data-channel='WORLD'] { border-left-color: #BA68C8; background: rgba(186,104,200,0.08); }
                .chat-row[data-channel='WORLD'] .nickname { color: #BA68C8; }

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

                .msg-wrapper { display: flex; align-items: flex-end; gap: 8px; }

                .msg-body {
                    position: relative; width: fit-content; max-width: 85%;
                    background: #252525; padding: 8px 12px;
                    border-radius: 0 12px 12px 12px; margin-top: 2px;
                    box-shadow: 0 2px 4px rgba(0,0,0,0.2);
                }

                .original { font-size: 0.9rem; line-height: 1.4; color: #ccc; }
                .translated {
                    color: #00ff88; margin-top: 4px;
                    font-size: 0.95rem; font-weight: 500;
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
                .chat-app.compact .msg-body { padding: 4px 8px; margin-top: 1px; border-radius: 4px; }

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
                "
            </style>
        </main>
    }
}