use indexmap::IndexMap;
use leptos::either::Either;
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
struct ProgressPayload {
    pub current: f64,
    pub total: f64,
    pub percent: u8,
}

#[component]
pub fn App() -> impl IntoView {
    // --- CORE SYSTEM SIGNALS ---
    let (status_text, set_status_text) = signal("Initializing...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);

    // --- CHAT & NAVIGATION SIGNALS ---
    let (active_tab, set_active_tab) = signal("전체".to_string());
    let (chat_log, set_chat_log) = signal(IndexMap::<u64, ChatPacket>::new());

    // --- UI INTERACTION SIGNALS ---
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

    // --- ACTIONS ---
    let setup_listeners = move || {
        spawn_local(async move {
            let packet_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(packet) = serde_json::from_value::<ChatPacket>(ev["payload"].clone()) {
                        let packet_clone = packet.clone();

                        set_chat_log.update(|log| {
                            // FIFO: shift_remove_index is O(n) but maintains insertion order
                            if log.len() >= 1000 {
                                log.shift_remove_index(0);
                            }
                            log.insert(packet.pid, packet);
                        });

                        if packet_clone.channel != "SYSTEM" && is_japanese(&packet_clone.message) {
                            spawn_local(async move {
                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                "text": packet_clone.message,
                                "pid": packet_clone.pid
                            })).unwrap();
                                let _ = invoke("manual_translate", args).await;
                            });
                        }
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            let trans_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(resp) = serde_json::from_str::<serde_json::Value>(ev["payload"].as_str().unwrap_or("")) {
                        let target_pid = resp["pid"].as_u64().unwrap_or(0);
                        let translated_text = resp["translated"].as_str().unwrap_or_default().to_string();

                        set_chat_log.update(|log| {
                            // 1. Find and Clone the existing packet
                            if let Some(packet) = log.get(&target_pid) {
                                let mut updated_packet = packet.clone();

                                // 2. Apply the update
                                updated_packet.translated = Some(translated_text);

                                // 3. RE-INSERT: This replaces the old value and signals a change
                                log.insert(target_pid, updated_packet);
                            }
                        });
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("packet-event", &packet_closure).await;
            listen("translator-event", &trans_closure).await;
            packet_closure.forget();
            trans_closure.forget();
        });
    };

    let start_download = move |_| {
        set_downloading.set(true);
        set_status_text.set("Starting Download...".to_string());

        spawn_local(async move {
            let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(wrapper) = serde_wasm_bindgen::from_value::<TauriEvent>(event_obj) {
                    let p = wrapper.payload;
                    set_progress.set(p.percent);
                    set_status_text.set(format!("Downloading AI Model... {}%", p.percent));
                }
            }) as Box<dyn FnMut(JsValue)>);

            let _ = listen("download-progress", &closure).await;
            closure.forget();

            match invoke("download_model", JsValue::NULL).await {
                Ok(_) => {
                    set_downloading.set(false);
                    set_model_ready.set(true);
                    set_status_text.set("Download Complete.".to_string());
                }
                Err(e) => {
                    set_downloading.set(false);
                    set_status_text.set(format!("Download Failed: {:?}", e));
                }
            }
        });
    };

    // --- STARTUP / EFFECTS ---
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(res) = invoke("check_model_status", JsValue::NULL).await {
                if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(res) {
                    if status.exists {
                        set_status_text.set("Loading History...".to_string());

                        // 1. Setup listeners first so we don't miss new packets
                        setup_listeners();

                        // 2. FETCH HISTORY FROM RUST (The Persistence Key)
                        if let Ok(history_res) = invoke("get_chat_history", JsValue::NULL).await {
                            // Hydrate the signal directly with the IndexMap
                            log!("History: {:?}", &history_res);
                            if let Ok(history_vec) = serde_wasm_bindgen::from_value::<Vec<ChatPacket>>(history_res) {
                                let history_map: IndexMap<u64, ChatPacket> = history_vec.into_iter().map(|p| (p.pid, p)).collect();
                                set_chat_log.set(history_map);
                            }
                        }

                        // 3. Start systems
                        let _ = invoke("start_sniffer_command", JsValue::NULL).await;

                        let trans_args = serde_wasm_bindgen::to_value(&serde_json::json!({ "useGpu": true })).unwrap();
                        let _ = invoke("start_translator_sidecar", trans_args).await;

                        set_model_ready.set(true);
                        set_status_text.set("Ready".to_string());
                    } else {
                        set_status_text.set("Model Missing".to_string());
                    }
                }
            }
        });
    });

    Effect::new(move |_| {
        chat_log.track();
        if !is_user_scrolling.get_untracked() {
            if let Some(el) = chat_container_ref.get() {
                el.set_scroll_top(el.scroll_height());
            }
        }
    });

    // --- UI VIEW ---
    let filtered_messages = Memo::new(move |_| {
        let tab = active_tab.get();
        let log = chat_log.get();

        // Convert IndexMap values to a Vec for filtering
        let messages: Vec<ChatPacket> = log.values().cloned().collect();

        if tab == "전체" {
            messages
        } else {
            let key = match tab.as_str() {
                "시스템" => "SYSTEM", "로컬" => "LOCAL", "파티" => "PARTY", "길드" => "GUILD", _ => "WORLD"
            };
            messages.into_iter().filter(|m| m.channel == key).collect()
        }
    });

    view! {
        <main class="chat-app">
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
                // CHATTING UI (Only shows when model is ready)
                <nav class="tab-bar">
                    {vec!["전체", "시스템", "로컬", "파티", "길드", "월드"].into_iter().map(|t| {
                        let t_name = t.to_string();
                        let t_click = t_name.clone();
                        view! {
                            <button class=move || if active_tab.get() == t_name { "tab-btn active" } else { "tab-btn" }
                                on:click=move |_| set_active_tab.set(t_click.clone())>{t}</button>
                        }
                    }).collect_view()}
                </nav>

                <div class="chat-container" node_ref=chat_container_ref
                    on:scroll=move |ev| {
                        let el = event_target::<HtmlDivElement>(&ev);
                        let bottom = el.scroll_top() + el.client_height() >= el.scroll_height() - 20;
                        set_user_scrolling.set(!bottom);
                    }>
                    <For each=move || filtered_messages.get()
                        key=|msg| msg.pid
                        children=move |msg| {
                            let is_jp = is_japanese(&msg.message);
                            let nickname = msg.nickname.clone();
                            let message = msg.message.clone();

                            view! {
                                <div class="chat-row" data-channel=msg.channel.clone()>
                                    <div class="msg-header">
                                        <span class="nickname">{nickname}</span>
                                        <span class="lvl">"Lv." {msg.level}</span>
                                        <span class="time">{format_time(msg.timestamp)}</span>
                                    </div>
                                    <div class="msg-body">
                                        <div class="original">
                                            {if is_jp { "[원문] " } else { "" }} {message}
                                        </div>

                                        // REACTIVE BLOCK: This closure re-executes when the 'msg'
                                        // associated with this PID is replaced in the IndexMap.
                                        {move || {
                                            msg.translated.clone().map(|text| {
                                                view! {
                                                    <div class="translated">
                                                        "[번역] " {text}
                                                    </div>
                                                }
                                            })
                                        }}
                                    </div>
                                </div>
                            }
                        }
                    />
                </div>
            </Show>

            <style>
                "
                .chat-app { display: flex; flex-direction: column; height: 100vh; background: #121212; font-family: sans-serif; color: #fff; }
                .setup-view { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; }
                .status-card { background: #1e1e1e; padding: 20px; border-radius: 8px; width: 350px; margin-bottom: 20px; text-align: center; }
                .progress-bar { width: 100%; height: 12px; background: #333; border-radius: 6px; overflow: hidden; margin-top: 10px; }
                .fill { height: 100%; background: #00ff88; transition: width 0.3s; }
                .primary-btn { background: #00ff88; color: #000; border: none; padding: 15px 30px; font-weight: bold; border-radius: 5px; cursor: pointer; }

                .tab-bar { display: flex; background: #1e1e1e; border-bottom: 1px solid #333; }
                .tab-btn { flex: 1; padding: 12px; border: none; background: none; color: #888; cursor: pointer; font-weight: bold; }
                .tab-btn.active { color: #00ff88; border-bottom: 2px solid #00ff88; background: #252525; }

                .chat-container { flex: 1; overflow-y: auto; padding: 10px; user-select: text; }
                .chat-row { margin-bottom: 12px; padding: 4px 8px; border-radius: 4px; border-left: 3px solid transparent; }

                .chat-row[data-channel='LOCAL'] { border-left-color: #E0E0E0; }
                .chat-row[data-channel='LOCAL'] .nickname { color: #E0E0E0; }
                .chat-row[data-channel='PARTY'] { border-left-color: #4FC3F7; }
                .chat-row[data-channel='PARTY'] .nickname { color: #4FC3F7; }
                .chat-row[data-channel='GUILD'] { border-left-color: #81C784; }
                .chat-row[data-channel='GUILD'] .nickname { color: #81C784; }
                .chat-row[data-channel='WORLD'] { border-left-color: #BA68C8; }
                .chat-row[data-channel='WORLD'] .nickname { color: #BA68C8; }
                .chat-row[data-channel='SYSTEM'] { border-left-color: #FFD54F; background: rgba(255, 213, 79, 0.05); }
                .msg-header { font-size: 0.85rem; display: flex; gap: 8px; color: #888; }
                .nickname { color: #ffcc00; font-weight: bold; }
                .translated { color: #00ff88; margin-top: 2px; font-size: 0.95rem; }
                "
            </style>
        </main>
    }
}