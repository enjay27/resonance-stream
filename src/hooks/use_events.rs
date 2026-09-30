use crate::chat_view::{is_muted, Tab};
use crate::service_state::SeqGate;
use crate::store::AppSignals;
use crate::tauri_bridge::{invoke, listen};
use crate::ui_types::{
    ChatMessage, ServiceStates, SnifferStatePayload, SystemMessage, TranslationResult,
    TranslatorStatePayload,
};
use leptos::logging::log;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

pub async fn clear_backend_history() {
    let _ = invoke("clear_chat_history", JsValue::NULL).await;
}

/// Listeners are registered once per page load: a second registration (the
/// setup wizard re-opening after a firewall error, then finishing) would
/// handle every event twice.
static LISTENERS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub async fn setup_event_listeners(signals: AppSignals) {
    if LISTENERS_REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    // 1. Create the closures using our new helper functions
    let packet_closure = create_packet_handler(signals);
    let system_closure = create_system_handler(signals);
    let translator_state_closure = create_translator_state_handler(signals);
    let sniffer_state_closure = create_sniffer_state_handler(signals);
    let translation_closure = create_translation_handler(signals);
    let update_message_closure = create_update_message_handler(signals);
    let firewall_closure = create_firewall_missing_handler(signals);

    // 2. Register all listeners
    listen("packet-event", &packet_closure).await;
    listen("system-event", &system_closure).await;
    listen("translator-state", &translator_state_closure).await;
    listen("sniffer-state", &sniffer_state_closure).await;
    listen("translation-event", &translation_closure).await;
    listen("chat-message-update", &update_message_closure).await;
    listen("firewall-missing", &firewall_closure).await;

    // 3. Prevent memory leaks / keep closures alive
    packet_closure.forget();
    system_closure.forget();
    translator_state_closure.forget();
    sniffer_state_closure.forget();
    translation_closure.forget();
    update_message_closure.forget();
    firewall_closure.forget();

    // 4. States emitted before we listened (the translator starts with the
    // app; a reloaded page missed everything): ask once, now that no later
    // event can be missed. The seq gates drop it if an event overtook it.
    if let Ok(res) = invoke("get_service_states", JsValue::NULL).await {
        if let Ok(states) = serde_wasm_bindgen::from_value::<ServiceStates>(res) {
            apply_sniffer_state(signals, states.sniffer);
            apply_translator_state(signals, states.translator);
        }
    }
}

static SNIFFER_SEQ: SeqGate = SeqGate::new();
static TRANSLATOR_SEQ: SeqGate = SeqGate::new();

fn apply_translator_state(signals: AppSignals, payload: TranslatorStatePayload) {
    if !TRANSLATOR_SEQ.accept(payload.seq) {
        return;
    }
    signals.set_translator_state.set(payload.state.clone());
    if payload.state == "Error" {
        signals.set_translator_error.set(payload.message);
    }
}

fn apply_sniffer_state(signals: AppSignals, payload: SnifferStatePayload) {
    if !SNIFFER_SEQ.accept(payload.seq) {
        return;
    }
    signals.set_sniffer_state.set(payload.state.clone());
    // If it's an error, save the message so the user can click the badge to read it
    if payload.state == "Error" {
        signals.set_sniffer_error.set(payload.message);
    }
}

// --- EXTRACTED HANDLER FUNCTIONS ---

#[derive(serde::Deserialize)]
struct TauriEvent<T> {
    payload: T,
}

/// Deserializes an event's payload straight from the JS value -- no
/// intermediate `serde_json::Value`, no copy of the payload.
fn payload<T: serde::de::DeserializeOwned>(event_obj: JsValue) -> Option<T> {
    serde_wasm_bindgen::from_value::<TauriEvent<T>>(event_obj)
        .ok()
        .map(|event| event.payload)
}

fn create_packet_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        // Stickers/emotes arrive already normalized by the backend.
        let Some(packet) = payload::<ChatMessage>(event_obj) else {
            return;
        };

        let limits = signals.tab_limits.get_untracked();
        // Blocked senders (and WORLD chat below the minimum level) never
        // ping and are not counted as unread.
        let muted = is_muted(&packet, signals.min_sender_level.get_untracked());
        let alert = !muted
            && signals
                .alert_keywords
                .with_untracked(|kws| kws.iter().any(|kw| packet.message.contains(kw.as_str())));
        let channel = packet.channel;
        let message_for_log = alert.then(|| packet.message.clone());

        let pid = packet.pid;
        signals.custom_filters.with_untracked(|filters| {
            signals.set_chat.update(|store| {
                store.add(pid, channel, ArcRwSignal::new(packet), filters, &limits);
            });
        });

        let tab = Tab::from_label(&signals.active_tab.get_untracked());
        let is_visible = signals
            .custom_filters
            .with_untracked(|filters| tab.shows_channel(channel, filters));

        // Only increment if the message belongs to the tab we are currently looking at
        if muted {
            // not unread: no badge for blocked or low-level senders
        } else if is_visible && !signals.is_at_bottom.get_untracked() {
            signals.set_unread_count.update(|c| *c += 1);
        } else if !is_visible {
            // Inactive tab -> Increment Tab Badge
            signals.set_unread_counts.update(|counts| {
                *counts.entry(channel.to_string()).or_insert(0) += 1;
            });
        }

        let volume = signals.alert_volume.get_untracked();
        if let Some(message) = message_for_log {
            // Fire and forget the audio ping
            if volume > 0.0 {
                log!("audio ping by keyword {:?}", message);
                if let Ok(audio) = web_sys::HtmlAudioElement::new_with_src("public/ping.mp3") {
                    // Convert f32 to f64 for the Web Audio API
                    audio.set_volume(volume as f64);
                    let _ = audio.play();
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_translation_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Some(payload) = payload::<TranslationResult>(event_obj) {
            // Find the existing message by PID and update its signal.
            // Only this row re-renders; the list itself is untouched.
            signals.chat.with_untracked(|store| {
                if let Some(chat_rw) = store.get(payload.pid) {
                    chat_rw.update(|c| {
                        c.translated = Some(payload.translated);
                    });
                }
            });
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_system_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        let Some(packet) = payload::<SystemMessage>(event_obj) else {
            return;
        };
        signals.set_system_log.update(|log| {
            if log.len() >= 200 {
                log.remove(0);
            }
            log.push(ArcRwSignal::new(packet));
        });

        let active_tab = signals.active_tab.get_untracked();
        if active_tab != "전체" && active_tab != "시스템" {
            signals.set_unread_counts.update(|counts| {
                *counts.entry("SYSTEM".to_string()).or_insert(0) += 1;
            });
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_translator_state_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Some(payload) = payload::<TranslatorStatePayload>(event_obj) {
            apply_translator_state(signals, payload);
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_sniffer_state_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Some(payload) = payload::<SnifferStatePayload>(event_obj) {
            apply_sniffer_state(signals, payload);
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_update_message_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        // The fully updated ChatMessage sent from the backend
        let Some(updated_msg) = payload::<ChatMessage>(event_obj) else {
            return;
        };
        // Find the existing signal by PID and completely overwrite its value
        signals.chat.with_untracked(|store| {
            if let Some(chat_rw) = store.get(updated_msg.pid) {
                chat_rw.set(updated_msg);
            }
        });
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_firewall_missing_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |_| {
        // 1. Force the Setup Wizard to appear
        signals.set_init_done.set(false);

        // 2. Make sure it starts on Step 0 (the Firewall Agreement page)
        signals.set_wizard_step.set(0);
    }) as Box<dyn FnMut(JsValue)>)
}
