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

    // --- SEPARATE DATA STREAMS ---
    // 1. Game Chat (IndexMap for updates)
    let (chat_log, set_chat_log) = signal(IndexMap::<u64, RwSignal<ChatPacket>>::new());
    // 2. System Logs (VecDeque logic in frontend)
    let (system_log, set_system_log) = signal(Vec::<RwSignal<ChatPacket>>::new());

    let (dict_update_available, set_dict_update_available) = signal(false);

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

    // --- ACTIONS ---
    let setup_listeners = move || {
        spawn_local(async move {
            // LISTENER 1: Game Packets
            let packet_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(packet) = serde_json::from_value::<ChatPacket>(ev["payload"].clone()) {
                        let packet_clone = packet.clone();

                        set_chat_log.update(|log| {
                            if log.len() >= 1000 { log.shift_remove_index(0); }
                            log.insert(packet.pid, RwSignal::new(packet));
                        });

                        // Trigger Translation ONLY for Game Chat
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

    Effect::new(move |_| {
        chat_log.track();
        if !is_user_scrolling.get_untracked() {
            if let Some(el) = chat_container_ref.get() { el.set_scroll_top(el.scroll_height()); }
        }
    });

    // --- OPTIMIZED VIEW LOGIC ---
    let filtered_messages = Memo::new(move |_| {
        let tab = active_tab.get();

        match tab.as_str() {
            // O(1): Return System Vector directly
            "시스템" => system_log.get(),

            // O(1): Return Game Vector directly (No filtering!)
            "전체" => chat_log.get().values().cloned().collect(),

            // O(N): Filter Game Vector for specific channels
            _ => {
                let key = match tab.as_str() {
                    "로컬" => "LOCAL", "파티" => "PARTY", "길드" => "GUILD", _ => "WORLD"
                };
                chat_log.get().values()
                    .filter(|m| m.get().channel == key)
                    .cloned()
                    .collect()
            }
        }
    });

    // 1. STATE: Track if the user is currently at the bottom
    let (is_at_bottom, set_is_at_bottom) = signal(true);
    let chat_container_ref = create_node_ref::<html::Div>();

    // 2. EFFECT: Auto-scroll when messages update
    Effect::new(move |_| {
        // We track 'filtered_messages' so this runs ONLY when the visible list changes
        filtered_messages.track();

        // Only auto-scroll if the user was ALREADY at the bottom
        if is_at_bottom.get_untracked() {
            // Use request_animation_frame to wait for the DOM to update with the new message
            request_animation_frame(move || {
                if let Some(el) = chat_container_ref.get() {
                    el.set_scroll_top(el.scroll_height());
                }
            });
        }
    });

    view! {
        <main class="chat-app">
            <Show when=move || model_ready.get() fallback=|| view! { <div class="setup-view">"..."</div> }>
                <nav class="tab-bar">
                    <div class="tabs">
                        {vec!["전체", "월드", "길드", "파티", "로컬", "시스템"].into_iter().map(|t| {
                            let t_name = t.to_string();
                            let t_click = t_name.clone();
                            let t_data = t_name.clone(); // Clone for data attribute
                            view! {
                                <button
                                    class=move || if active_tab.get() == t_name { "tab-btn active" } else { "tab-btn" }
                                    data-tab=t_data // <--- ADD THIS
                                    on:click=move |_| set_active_tab.set(t_click.clone())
                                >
                                    {t}
                                </button>
                            }
                        }).collect_view()}
                    </div>

                    // --- DICTIONARY SYNC BUTTON ---
                    <div class="dict-sync-area">
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

                <div class="chat-container" node_ref=chat_container_ref
                    // 3. EVENT: Detect manual scrolling
                    on:scroll=move |ev| {
                        let el = event_target::<HtmlDivElement>(&ev);
                        // "Tolerance" of 30px allows for minor pixel differences
                        let at_bottom = el.scroll_height() - el.scroll_top() - el.client_height() < 30;
                        set_is_at_bottom.set(at_bottom);
                    }>
                    <For each=move || filtered_messages.get()
                        key=|sig| sig.get_untracked().pid
                        children=move |sig| {
                            // This child closure now receives an individual RwSignal
                            let msg = sig.get();
                            let is_jp = is_japanese(&msg.message);

                            view! {
                                <div class="chat-row" data-channel=move || sig.get().channel.clone()>
                                    <div class="msg-header">
                                        <span class="nickname">{move || sig.get().nickname.clone()}</span>
                                        <span class="lvl">"Lv." {move || sig.get().level}</span>
                                        <span class="time">{format_time(msg.timestamp)}</span>
                                    </div>
                                    <div class="msg-wrapper">

                                        // The Message Bubble
                                        <div class="msg-body">
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
                /* --- APP LAYOUT --- */
                .chat-app { display: flex; flex-direction: column; height: 100vh; background: #121212; font-family: sans-serif; color: #fff; }
                .setup-view { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; }
                .status-card { background: #1e1e1e; padding: 20px; border-radius: 8px; width: 350px; margin-bottom: 20px; text-align: center; }
                .progress-bar { width: 100%; height: 12px; background: #333; border-radius: 6px; overflow: hidden; margin-top: 10px; }
                .fill { height: 100%; background: #00ff88; transition: width 0.3s; }
                .primary-btn { background: #00ff88; color: #000; border: none; padding: 15px 30px; font-weight: bold; border-radius: 5px; cursor: pointer; }

                /* --- TAB BAR (Adaptive Colors) --- */
                .tab-bar { display: flex; justify-content: space-between; align-items: center; background: #1e1e1e; border-bottom: 1px solid #333; }
                .tabs { display: flex; flex: 1; }

                .tab-btn {
                    flex: 1; padding: 12px; border: none; background: none;
                    cursor: pointer; font-weight: bold; transition: all 0.2s;
                    opacity: 0.6; border-bottom: 2px solid transparent;
                }
                .tab-btn:hover, .tab-btn.active { opacity: 1; background: #252525; }

                /* Tab Specific Colors */
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


                /* --- CHAT ROWS (Distinct Background Tints) --- */
                .chat-container { flex: 1; overflow-y: auto; padding: 10px; user-select: text; }

                .chat-row {
                    margin-bottom: 8px;
                    padding: 6px 10px;
                    border-radius: 4px;
                    border-left: 3px solid transparent;
                    /* Default Text Color */
                    color: #ddd;
                }
                .copy-btn {
                    /* No absolute positioning needed anymore */
                    background: transparent;
                    border: none;
                    color: #555;
                    cursor: pointer;
                    font-size: 1.1rem; /* Slightly larger icon since it's outside */
                    padding: 4px;
                    border-radius: 4px;

                    /* Hidden until you hover the row */
                    opacity: 0;
                    transition: all 0.2s;
                }
                /* Show button when hovering the WRAPPER (Bubble area) */
                .msg-wrapper:hover .copy-btn {
                    opacity: 1;
                }
                .copy-btn:hover {
                    color: #00ff88;
                    background: rgba(255, 255, 255, 0.05);
                    transform: scale(1.1);
                }
                .copy-btn:active {
                    transform: scale(0.95);
                }

                /* 1. LOCAL (Gray/White) */
                .chat-row[data-channel='LOCAL'] {
                    border-left-color: #BDBDBD;
                    background: rgba(189, 189, 189, 0.05); /* Subtle Gray Tint */
                }
                .chat-row[data-channel='LOCAL'] .nickname { color: #BDBDBD; }

                /* 2. PARTY (Blue) */
                .chat-row[data-channel='PARTY'] {
                    border-left-color: #4FC3F7;
                    background: rgba(79, 195, 247, 0.08); /* Subtle Blue Tint */
                }
                .chat-row[data-channel='PARTY'] .nickname { color: #4FC3F7; }

                /* 3. GUILD (Green) */
                .chat-row[data-channel='GUILD'] {
                    border-left-color: #81C784;
                    background: rgba(129, 199, 132, 0.08); /* Subtle Green Tint */
                }
                .chat-row[data-channel='GUILD'] .nickname { color: #81C784; }

                /* 4. WORLD (Purple) */
                .chat-row[data-channel='WORLD'] {
                    border-left-color: #BA68C8;
                    background: rgba(186, 104, 200, 0.08); /* Subtle Purple Tint */
                }
                .chat-row[data-channel='WORLD'] .nickname { color: #BA68C8; }

                /* 5. SYSTEM (Yellow) */
                .chat-row[data-channel='SYSTEM'] {
                    border-left-color: #FFD54F;
                    background: rgba(255, 213, 79, 0.08); /* Subtle Yellow Tint */
                }
                /* System messages usually don't have a nickname, or it's "SYSTEM" */
                .chat-row[data-channel='SYSTEM'] .nickname { color: #FFD54F; }


                /* --- TEXT STYLING --- */
                .msg-header {
                    display: flex;
                    align-items: baseline; /* Aligns text by their bottom line (better for different sizes) */
                    gap: 8px;
                    margin-bottom: 4px;
                    opacity: 0.9;
                }

                .nickname {
                    font-size: 1.05rem; /* Bigger than the standard text */
                    font-weight: 600;   /* Extra Bold */
                    letter-spacing: 0.5px;
                    /* Color is set by the data-channel rules above, so we don't force it here */
                }

                .lvl {
                    font-size: 0.75rem; /* Keep metadata small */
                    color: #888;
                }

                .time {
                    font-size: 0.75rem;
                    color: #666;
                    margin-left: auto;
                }

                .msg-wrapper {
                    display: flex;
                    align-items: flex-end; /* Vertically aligns button with the middle of the message */
                    gap: 8px;            /* Space between bubble and button */
                    width: 100%;
                }
                .msg-body {
                    position: relative;
                    display: flex;
                    flex-direction: column;

                    width: fit-content;
                    max-width: 85%; /* Slightly reduced to make room for button */

                    background: #252525;
                    padding: 8px 12px; /* Standard padding (no extra space needed now) */
                    border-radius: 0 12px 12px 12px;
                    margin-top: 4px;
                    box-shadow: 0 2px 4px rgba(0,0,0,0.2);
                    border-left: 3px solid transparent;
                }
                .msg-body:hover .copy-btn {
                    opacity: 1;
                }
                .original { font-size: 0.95rem; line-height: 1.4; }
                .translated {
                    color: #00ff88;
                    margin-top: 4px;
                    font-size: 0.95rem;
                    font-weight: 500;
                }

                /* --- UTILS --- */
                .dict-sync-area { padding-right: 15px; position: relative; }
                .sync-btn {
                    background: #333; color: #aaa; border: 1px solid #444;
                    padding: 5px 12px; border-radius: 4px; font-size: 0.75rem;
                    cursor: pointer; position: relative; transition: all 0.2s;
                }
                .sync-btn:hover { background: #444; color: #fff; }
                .sync-btn:disabled { opacity: 0.5; cursor: not-allowed; }

                .update-dot {
                    position: absolute; top: -4px; right: -4px;
                    width: 8px; height: 8px; background: #ff4444;
                    border-radius: 50%; border: 2px solid #1e1e1e;
                    animation: pulse 2s infinite;
                }
                @keyframes pulse {
                    0% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(255, 68, 68, 0.7); }
                    70% { transform: scale(1); box-shadow: 0 0 0 6px rgba(255, 68, 68, 0); }
                    100% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(255, 68, 68, 0); }
                }
                "
            </style>
        </main>
    }
}