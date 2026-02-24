use crate::store::AppSignals;
use crate::tauri_bridge::{invoke, listen};
use crate::types::{ChatMessage, SystemMessage};
use crate::utils::is_japanese;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

pub async fn clear_backend_history() {
    let _ = invoke("clear_chat_history", JsValue::NULL).await;
}

pub async fn setup_event_listeners(signals: AppSignals) {
    // PACKET LISTENER: Handles incoming Blue Protocol chat
    let packet_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            if let Ok(mut packet) = serde_json::from_value::<ChatMessage>(ev["payload"].clone()) {
                // Handle Stickers/Emojis
                if packet.message.starts_with("emojiPic=") { packet.message = "스티커 전송".to_string(); }

                signals.set_chat_log.update(|log| {
                    let limit = signals.chat_limit.get_untracked();
                    if log.len() >= limit {
                        log.shift_remove_index(0);
                    }
                    log.insert(packet.pid, RwSignal::new(packet.clone()));
                });

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
    }) as Box<dyn FnMut(JsValue)>);

    // SYSTEM LISTENER: Handles app logs
    let system_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
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
    }) as Box<dyn FnMut(JsValue)>);

    listen("packet-event", &packet_closure).await;
    listen("system-event", &system_closure).await;

    packet_closure.forget();
    system_closure.forget();
}