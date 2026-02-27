use leptos::logging::log;
use crate::store::AppSignals;
use crate::tauri_bridge::{invoke, listen};
use crate::types::{ChatMessage, SnifferStatePayload, SystemMessage};
use crate::utils::is_japanese;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

pub async fn clear_backend_history() {
    let _ = invoke("clear_chat_history", JsValue::NULL).await;
}

pub async fn setup_event_listeners(signals: AppSignals) {
    // 1. Create the closures using our new helper functions
    let packet_closure = create_packet_handler(signals);
    let system_closure = create_system_handler(signals);
    let sniffer_state_closure = create_sniffer_state_handler(signals);

    // 2. Register all listeners
    listen("packet-event", &packet_closure).await;
    listen("system-event", &system_closure).await;
    listen("sniffer-state", &sniffer_state_closure).await;

    // 3. Prevent memory leaks / keep closures alive
    packet_closure.forget();
    system_closure.forget();
    sniffer_state_closure.forget();
}

// --- EXTRACTED HANDLER FUNCTIONS ---

fn create_packet_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            if let Ok(mut packet) = serde_json::from_value::<ChatMessage>(ev["payload"].clone()) {

                // Handle Stickers/Emojis
                if packet.message.starts_with("emojiPic=") {
                    packet.message = "스티커 전송".to_string();
                    packet.translated = None;
                }
                if packet.message.starts_with("<sprite=") {
                    packet.message = "이모지 전송".to_string();
                    packet.translated = None;
                }

                signals.set_chat_log.update(|log| {
                    let limit = signals.chat_limit.get_untracked();
                    if log.len() >= limit {
                        log.shift_remove_index(0);
                    }
                    log.insert(packet.pid, RwSignal::new(packet.clone()));
                });

                let active_tab = signals.active_tab.get_untracked();
                let is_visible = match active_tab.as_str() {
                    "전체" => true,
                    "커스텀" => signals.custom_filters.get_untracked().contains(&packet.channel),
                    "시스템" => false,
                    _ => {
                        let key = match active_tab.as_str() {
                            "로컬" => "LOCAL", "파티" => "PARTY", "길드" => "GUILD", _ => "WORLD"
                        };
                        packet.channel == key
                    }
                };

                // Only increment if the message belongs to the tab we are currently looking at
                if is_visible && !signals.is_at_bottom.get_untracked() {
                    signals.set_unread_count.update(|c| *c += 1);
                }

                let pid = packet.pid;
                let nickname = packet.nickname.clone();

                // NICKNAME STRATEGY: Check Cache -> Request if Missing
                let cached_nickname = signals.name_cache.with(|cache| cache.get(&nickname).cloned());

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

                // Auto-Translate Logic
                if is_japanese(&packet.message) && signals.use_translation.get_untracked() {
                    let pid = packet.pid;
                    let msg = packet.message.clone();
                    spawn_local(async move {
                        let _ = invoke("translate_message", serde_wasm_bindgen::to_value(&serde_json::json!({
                            "text": msg, "pid": pid, "nickname": None::<String>
                        })).unwrap()).await;
                    });
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_system_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            if let Ok(packet) = serde_json::from_value::<SystemMessage>(ev["payload"].clone()) {
                signals.set_system_log.update(|log| {
                    if log.len() >= 200 {
                        log.remove(0);
                    }
                    log.push(RwSignal::new(packet));
                });
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_sniffer_state_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            if let Ok(payload) = serde_json::from_value::<SnifferStatePayload>(ev["payload"].clone()) {
                signals.set_sniffer_state.set(payload.state.clone());

                // If it's an error, save the message so the user can click the badge to read it
                if payload.state == "Error" {
                    signals.set_sniffer_error.set(payload.message);
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}