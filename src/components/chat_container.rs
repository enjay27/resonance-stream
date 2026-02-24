use crate::components::ChatRow;
use crate::store::AppSignals;
use crate::types::{ChatMessage, SystemMessage};
use crate::utils::format_time;
use leptos::html;
use leptos::prelude::*;
use web_sys::HtmlDivElement;

#[component]
pub fn ChatContainer() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let chat_container_ref = create_node_ref::<html::Div>();

    // --- FILTERED VIEW LOGIC (Restored from app.rs) ---
    let filtered_chat = Memo::new(move |_| {
        let tab = signals.active_tab.get();
        let search = signals.search_term.get().to_lowercase();
        let filters = signals.custom_filters.get();
        let chat_log = signals.chat_log.get();

        if tab == "시스템" { return Vec::new(); }

        let base_list = match tab.as_str() {
            "전체" => chat_log.values().cloned().collect::<Vec<_>>(),
            "커스텀" => chat_log.values()
                .filter(|m| filters.contains(&m.get().channel))
                .cloned().collect(),
            _ => {
                let key = match tab.as_str() {
                    "로컬" => "LOCAL", "파티" => "PARTY", "길드" => "GUILD", _ => "WORLD"
                };
                chat_log.values()
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
        let logs = signals.system_log.get();
        let level_f = signals.system_level_filter.get();
        let source_f = signals.system_source_filter.get();
        let search = signals.search_term.get().to_lowercase();
        let debug_enabled = signals.is_debug.get();

        logs.into_iter().filter(|sig| {
            let m = sig.get();
            if !debug_enabled && m.level == "debug" { return false; }
            let matches_level = level_f.as_ref().map_or(true, |f| &m.level == f);
            let matches_source = source_f.as_ref().map_or(true, |f| &m.source == f);
            matches_level && matches_source && (search.is_empty() || m.message.to_lowercase().contains(&search))
        }).collect::<Vec<_>>()
    });

    // --- SCROLL LOGIC ---
    Effect::new(move |_| {
        filtered_chat.track();
        if signals.is_at_bottom.get_untracked() {
            request_animation_frame(move || {
                if let Some(el) = chat_container_ref.get() {
                    el.set_scroll_top(el.scroll_height());
                }
            });
        }
    });

    view! {
        <div class="relative flex-1 min-h-0 bg-black/20"
            node_ref=chat_container_ref
            on:scroll=move |ev| {
                let el = event_target::<HtmlDivElement>(&ev);
                let at_bottom = el.scroll_height() - el.scroll_top() - el.client_height() < 15;
                if signals.active_tab.get_untracked() == "시스템" {
                    signals.set_system_at_bottom.set(at_bottom);
                } else {
                    signals.set_is_at_bottom.set(at_bottom);
                    if at_bottom { signals.set_unread_count.set(0); }
                }
            }
        >
            // --- SCROLLABLE CONTENT ---
            <div class="overflow-y-auto h-full custom-scrollbar p-2">
                <Show
                    when=move || signals.active_tab.get() == "시스템"
                    fallback=move || view! {
                        <For
                            each=move || filtered_chat.get()
                            key=|sig| sig.get_untracked().pid
                            children=move |sig| view! { <ChatRow sig=sig /> }
                        />
                    }
                >
                    <For
                        each=move || filtered_system_logs.get()
                        key=|sig| sig.get_untracked().pid
                        children={move |sig: RwSignal<SystemMessage>| {
                            let level = sig.get().level.clone();
                            view! {
                                <div class="chat chat-start opacity-80 mb-1">
                                    <div class=move || format!("chat-bubble chat-bubble-xs border border-white/5 bg-base-300 min-h-0 text-[11px] leading-tight {}",
                                        match level.as_str() {
                                            "error" => "text-error",
                                            "warning" => "text-warning",
                                            "success" => "text-success",
                                            _ => "text-gray-400"
                                        }
                                    )>
                                        <span class="font-black mr-2">"[" {move || sig.get().source.to_uppercase()} "]"</span>
                                        {move || sig.get().message.clone()}
                                        <span class="ml-2 opacity-30 text-[9px]">{move || format_time(sig.get().timestamp)}</span>
                                    </div>
                                </div>
                            }
                        }}
                    />
                />
            </Show>
            </div>

            // --- OVERLAY: NEW MESSAGE TOAST ---
            <Show when=move || signals.unread_count.get().gt(&0) >
                <div class="absolute bottom-4 left-1/2 -translate-x-1/2 z-50">
                    <button class="btn btn-success btn-sm shadow-lg gap-2 animate-bounce"
                        on:click=move |_| {
                            if let Some(el) = chat_container_ref.get() {
                                el.set_scroll_top(el.scroll_height());
                            }
                        }>
                        <span class="badge badge-ghost badge-sm">{move || signals.unread_count.get()}</span>
                        "새로운 메시지"
                    </button>
                </div>
            </Show>

            // --- OVERLAY: SCROLL LOCK TOAST ---
            <Show when=move || signals.active_tab.get() == "시스템" && !signals.is_system_at_bottom.get()>
                <div class="absolute bottom-4 left-1/2 -translate-x-1/2 z-50">
                    <button class="btn btn-warning btn-sm opacity-90 shadow-lg"
                        on:click=move |_| {
                            if let Some(el) = chat_container_ref.get() {
                                el.set_scroll_top(el.scroll_height());
                                signals.set_system_at_bottom.set(true);
                            }
                        }>
                        "⬆️ Scroll Locked"
                    </button>
                </div>
            </Show>
        </div>
    }
}