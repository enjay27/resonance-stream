use leptos::html;
// src-ui/src/app.rs
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[derive(PartialEq)]
pub struct ChatPacket {
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

#[component]
pub fn App() -> impl IntoView {
    // --- CORE SYSTEM SIGNALS ---
    let (status_text, set_status_text) = signal("System Check...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, _set_downloading) = signal(false);
    let (_progress, _set_progress) = signal(0u8);

    // --- CHAT & NAVIGATION SIGNALS ---
    let (active_tab, set_active_tab) = signal("전체".to_string());
    let (chat_log, set_chat_log) = signal(Vec::<ChatPacket>::new());

    // --- UI INTERACTION SIGNALS ---
    let (is_user_scrolling, set_user_scrolling) = signal(false);
    let chat_container_ref = create_node_ref::<html::Div>();

    // --- DERIVED SIGNALS ---

    let filtered_messages = Memo::new(move |_| {
        let tab = active_tab.get();
        let log = chat_log.get();
        if tab == "전체" {
            log
        } else {
            let channel_key = match tab.as_str() {
                "로컬" => "LOCAL",
                "파티" => "PARTY",
                "길드" => "GUILD",
                "월드" => "WORLD",
                _ => "SYSTEM",
            };
            log.into_iter().filter(|m| m.channel == channel_key).collect()
        }
    });

    // --- HELPERS ---

    let format_time = |ts: u64| {
        let date = js_sys::Date::new(&JsValue::from_f64(ts as f64 * 1000.0));
        format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
    };

    let is_japanese = |text: &str| {
        let re = js_sys::RegExp::new("[\\u3040-\\u309F\\u30A0-\\u30FF\\u4E00-\\u9FAF]", "");
        re.test(text)
    };

    // --- EVENT LISTENERS ---

    let setup_listeners = move || {
        spawn_local(async move {
            // 1. Listen for new packets from the Sniffer
            let packet_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(packet) = serde_json::from_value::<ChatPacket>(ev["payload"].clone()) {
                        let packet_clone = packet.clone();

                        set_chat_log.update(|log| log.push(packet));

                        // 2. TRIGGER TRANSLATION IF JAPANESE
                        if is_japanese(&packet_clone.message) {
                            spawn_local(async move {
                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "text": packet_clone.message })).unwrap();
                                let _ = invoke("manual_translate", args).await;
                            });
                        }
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            // 2. Listen for translation results from the AI Sidecar
            let trans_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Some(json_str) = event_obj.as_string() {
                    if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        let original = resp["original"].as_str().unwrap_or_default().to_string();
                        let translated = resp["translated"].as_str().unwrap_or_default().to_string();

                        set_chat_log.update(|log| {
                            if let Some(msg) = log.iter_mut().rev().find(|m| m.message == original) {
                                msg.translated = Some(translated);
                            }
                        });
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("new-chat-message", &packet_closure).await;
            listen("translator-event", &trans_closure).await;
            packet_closure.forget();
            trans_closure.forget();
        });
    };

    // --- AUTO SCROLL LOGIC ---
    Effect::new(move |_| {
        chat_log.track();
        if !is_user_scrolling.get_untracked() {
            if let Some(el) = chat_container_ref.get() {
                el.set_scroll_top(el.scroll_height());
            }
        }
    });

    // --- STARTUP LOGIC ---
    Effect::new(move |_| {
        let set_status = set_status_text;
        let set_ready = set_model_ready;

        spawn_local(async move {
            if let Ok(res) = invoke("check_model_status", JsValue::NULL).await {
                if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(res) {
                    set_ready.set(status.exists);

                    if status.exists {
                        set_status.set("AI Sidecar Booting...".to_string());
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "useGpu": true })).unwrap();
                        let _ = invoke("start_translator_sidecar", args).await;
                        set_status.set("AI Engine Ready".to_string());
                    } else {
                        set_status.set("Model Missing: Download Required".to_string());
                    }
                }
            }
            // CRITICAL: Call the listeners after initialization logic
            setup_listeners();
        });
    });

    view! {
        <main class="chat-app">
            <nav class="tab-bar">
            {vec!["전체", "로컬", "파티", "길드", "월드"].into_iter().map(|t| {
                let tab_name = t.to_string();
                let tab_name_for_click = tab_name.clone();

                view! {
                    <button
                        class=move || if active_tab.get() == tab_name { "tab-btn active" } else { "tab-btn" }
                        on:click=move |_| set_active_tab.set(tab_name_for_click.clone())
                    >
                        {t}
                    </button>
                }
            }).collect_view()}
            </nav>

            <div
                class="chat-container"
                node_ref=chat_container_ref
                on:scroll=move |ev| {
                    let el = event_target::<HtmlDivElement>(&ev);
                    let at_bottom = el.scroll_top() + el.client_height() >= el.scroll_height() - 20;
                    set_user_scrolling.set(!at_bottom);
                }
            >
            <For
                each=move || filtered_messages.get()
                // Update key to include translation state to force rerender
                key=|msg| format!("{}-{}-{}", msg.timestamp, msg.uid, msg.translated.is_some())
                children=move |msg| {
                    let is_jp = is_japanese(&msg.message);
                    let translated_base = msg.translated.clone();

                    view! {
                        <div class="chat-row" data-channel=msg.channel.clone()>
                            <div class="msg-header">
                                <span class="nickname">{msg.nickname.clone()}</span>
                                <span class="lvl">"Lv." {msg.level}</span>
                                <span class="time">{format_time(msg.timestamp)}</span>
                            </div>
                            <div class="msg-body">
                                <div class="original">
                                    {if is_jp { "[원문] " } else { "" }}
                                    {msg.message.clone()}
                                </div>

                                {
                                    let translated_when = translated_base.clone();
                                    let translated_child = translated_base.clone();

                                    view! {
                                        <Show when=move || translated_when.is_some()>
                                            <div class="translated">
                                                "[번역] "
                                                {
                                                    let translated_final = translated_child.clone();
                                                    move || translated_final.clone().unwrap_or_default()
                                                }
                                            </div>
                                        </Show>
                                    }
                                }
                            </div>
                        </div>
                    }
                }
            />
            </div>

            <style>
                "
                .chat-app { display: flex; flex-direction: column; height: 100vh; background: #121212; font-family: sans-serif; }
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

                .msg-header { font-size: 0.85rem; display: flex; gap: 8px; margin-bottom: 2px; align-items: center; }
                .lvl { color: #888; font-size: 0.75rem; }
                .time { margin-left: auto; color: #555; font-size: 0.75rem; }
                .original { color: #eee; line-height: 1.4; }
                .translated { color: #00ff88; font-size: 0.95rem; margin-top: 2px; }
                "
            </style>
        </main>
    }
}