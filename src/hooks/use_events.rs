use crate::store::AppSignals;
use crate::tauri_bridge::{invoke, listen};
use crate::ui_types::{ChatMessage, SnifferStatePayload, SystemMessage, TranslationResult};
use leptos::logging::log;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

pub async fn clear_backend_history() {
    let _ = invoke("clear_chat_history", JsValue::NULL).await;
}

pub async fn setup_event_listeners(signals: AppSignals) {
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
}

// --- EXTRACTED HANDLER FUNCTIONS ---

fn create_packet_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            match serde_json::from_value::<ChatMessage>(ev["payload"].clone()) {
                Ok(mut packet) => {
                    log!("Successfully parsed packet: {:?}", packet);

                    // Handle Stickers/Emojis
                    if packet.message.starts_with("emojiPic=") {
                        packet.message = "[스티커]".to_string();
                        packet.translated = None;
                    }
                    if packet.message.contains("<sprite=") {
                        let mut output = String::with_capacity(packet.message.len());
                        let mut current = packet.message.as_str();

                        while let Some(start) = current.find("<sprite=") {
                            // Push the text *before* the sprite tag
                            output.push_str(&current[..start]);

                            // Find the closing '>'
                            if let Some(end) = current[start..].find('>') {
                                // Insert our clean UI placeholder
                                output.push_str("[이모지]");
                                // Move the cursor past the '>'
                                current = &current[start + end + 1..];
                            } else {
                                // If the tag is somehow broken/malformed, stop parsing
                                output.push_str(&current[start..]);
                                current = "";
                                break;
                            }
                        }
                        // Push any remaining text *after* the last sprite tag
                        output.push_str(current);
                        packet.message = output;
                        packet.translated = None;
                    }

                    let pid = packet.pid;
                    let ch = packet.channel.clone();
                    let limits = signals.tab_limits.get_untracked();
                    let filters = signals.custom_filters.get_untracked();

                    let aggregate_limit: usize = limits.iter()
                        .filter(|(k, _)| *k != "전체" && *k != "커스텀" && *k != "SYSTEM")
                        .map(|(_, v)| *v)
                        .sum();
                    // Fallback to 2000 just in case all limits were somehow set to 0
                    let aggregate_limit = if aggregate_limit == 0 { 2000 } else { aggregate_limit };

                    // 1. Add to DB
                    signals.set_chat_db.update(|db| {
                        db.insert(pid, RwSignal::new(packet.clone()));
                    });

                    // 2. Update Views
                    signals.set_tab_views.update(|tabs| {
                        // All Tab (Uses dynamic aggregate limit)
                        let all_tab = tabs.entry("전체".to_string()).or_insert_with(std::collections::VecDeque::new);
                        all_tab.push_back(pid);
                        while all_tab.len() > aggregate_limit { all_tab.pop_front(); }

                        // Specific Channel Tab
                        let spec_limit = *limits.get(&ch).unwrap_or(&500);
                        let spec_tab = tabs.entry(ch.clone()).or_insert_with(std::collections::VecDeque::new);
                        spec_tab.push_back(pid);
                        while spec_tab.len() > spec_limit { spec_tab.pop_front(); }

                        // Custom Tab (Uses dynamic aggregate limit)
                        if filters.contains(&ch) {
                            let custom_tab = tabs.entry("커스텀".to_string()).or_insert_with(std::collections::VecDeque::new);
                            custom_tab.push_back(pid);
                            while custom_tab.len() > aggregate_limit { custom_tab.pop_front(); }
                        }
                    });

                    // 3. Garbage Collection (Deletes messages safely from RAM if no tabs are looking at them)
                    signals.set_chat_db.update(|db| {
                        let active_tabs = signals.tab_views.get_untracked();
                        db.retain(|db_pid, _| active_tabs.values().any(|pid_list| pid_list.contains(db_pid)));
                    });

                    let active_tab = signals.active_tab.get_untracked();
                    let is_visible = match active_tab.as_str() {
                        "전체" => true,
                        "커스텀" => signals
                            .custom_filters
                            .get_untracked()
                            .contains(&packet.channel),
                        "시스템" => false,
                        _ => {
                            let key = match active_tab.as_str() {
                                "로컬" => "LOCAL",
                                "파티" => "PARTY",
                                "길드" => "GUILD",
                                _ => "WORLD",
                            };
                            packet.channel == key
                        }
                    };

                    // Only increment if the message belongs to the tab we are currently looking at
                    if is_visible && !signals.is_at_bottom.get_untracked() {
                        signals.set_unread_count.update(|c| *c += 1);
                    } else if !is_visible {
                        // Inactive tab -> Increment Tab Badge
                        signals.set_unread_counts.update(|counts| {
                            *counts.entry(packet.channel.clone()).or_insert(0) += 1;
                        });
                    }

                    let keywords = signals.alert_keywords.get_untracked();
                    let volume = signals.alert_volume.get_untracked();

                    if keywords.iter().any(|kw| packet.message.contains(kw)) {
                        // Fire and forget the audio ping
                        if volume > 0.0 {
                            log!("audio ping by keyword {:?}", packet.message);
                            if let Ok(audio) =
                                web_sys::HtmlAudioElement::new_with_src("public/ping.mp3")
                            {
                                // Convert f32 to f64 for the Web Audio API
                                audio.set_volume(volume as f64);
                                let _ = audio.play();
                            }
                        }
                    }
                }
                Err(e) => {
                    // This will now catch any Type Mismatches in the future!
                    log!("❌ DESERIALIZATION ERROR: {:?}", e);
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_translation_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            if let Ok(payload) = serde_json::from_value::<TranslationResult>(ev["payload"].clone())
            {
                // FIX: Use chat_db instead of chat_log
                signals.set_chat_db.update(|db| {
                    if let Some(chat_rw) = db.get(&payload.pid) {
                        chat_rw.update(|c| {
                            c.translated = Some(payload.translated);
                        });
                    }
                });
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

                let active_tab = signals.active_tab.get_untracked();
                if active_tab != "전체" && active_tab != "시스템" {
                    signals.set_unread_counts.update(|counts| {
                        *counts.entry("SYSTEM".to_string()).or_insert(0) += 1;
                    });
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_translator_state_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            // NOTE: Make sure to import TranslatorStatePayload at the top of use_events.rs!
            if let Ok(payload) = serde_json::from_value::<crate::ui_types::TranslatorStatePayload>(
                ev["payload"].clone(),
            ) {
                signals.set_translator_state.set(payload.state.clone());
                if payload.state == "Error" {
                    signals.set_translator_error.set(payload.message);
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_sniffer_state_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            if let Ok(payload) =
                serde_json::from_value::<SnifferStatePayload>(ev["payload"].clone())
            {
                signals.set_sniffer_state.set(payload.state.clone());

                // If it's an error, save the message so the user can click the badge to read it
                if payload.state == "Error" {
                    signals.set_sniffer_error.set(payload.message);
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>)
}

fn create_update_message_handler(signals: AppSignals) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
            if let Ok(updated_msg) = serde_json::from_value::<ChatMessage>(ev["payload"].clone()) {
                // FIX: Use chat_db instead of chat_log
                signals.set_chat_db.update(|db| {
                    if let Some(chat_rw) = db.get(&updated_msg.pid) {
                        chat_rw.set(updated_msg);
                    }
                });
            }
        }
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