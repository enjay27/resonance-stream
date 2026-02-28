use crate::components::ChatRow;
use crate::store::AppSignals;
use crate::types::{ChatMessage, SystemMessage};
use crate::utils::format_time;
use leptos::html;
use leptos::leptos_dom::log;
use leptos::prelude::*;
use web_sys::{HtmlDivElement, MouseEvent};

#[component]
pub fn ChatContainer() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let chat_container_ref = create_node_ref::<html::Div>();

    // Start by only rendering the last 50 messages to keep the DOM blazing fast
    let (display_limit, set_display_limit) = signal(50);

    // DRAG TO SCROLL STATE ---
    let (is_dragging, set_is_dragging) = signal(false);
    let (start_y, set_start_y) = signal(0);
    let (saved_scroll_top, set_saved_scroll_top) = signal(0);

    Effect::new(move |_| {
        signals.active_tab.track();
        signals.search_term.track();
        set_display_limit.set(50);
    });

    // --- FILTERED VIEW LOGIC ---
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

        let full_list: Vec<_> = if search.is_empty() { base_list }
        else {
            base_list.into_iter().filter(|sig| {
                let m = sig.get();
                m.nickname.to_lowercase().contains(&search) || m.message.to_lowercase().contains(&search)
            }).collect()
        };

        // --- SLICE THE LIST (PAGING) ---
        let current_limit = display_limit.get();
        let total = full_list.len();

        // Only return the bottom `current_limit` amount of messages
        if total > current_limit {
            full_list[total - current_limit..].to_vec()
        } else {
            full_list
        }
    });

    let filtered_system_logs = Memo::new(move |_| {
        let logs = signals.system_log.get();
        let level_f = signals.system_level_filter.get();
        let source_f = signals.system_source_filter.get();
        let search = signals.search_term.get().to_lowercase();
        let current_log_level = signals.log_level.get();

        logs.into_iter().filter(|sig| {
            let m = sig.get();

            // 1. Assign numeric values to create a hierarchy
            let msg_val = match m.level.as_str() {
                "trace" => 0,
                "debug" => 1,
                "info" | "success" => 2,
                "warn" => 3,
                "error" => 4,
                _ => 2, // default unknown to info
            };

            let filter_val = match current_log_level.as_str() {
                "trace" => 0,
                "debug" => 1,
                "info" => 2,
                "warn" => 3,
                "error" => 4,
                _ => 2,
            };

            // 2. Hide messages that are beneath the chosen log level
            if msg_val < filter_val { return false; }

            // 3. Apply standard UI filters
            let matches_level = level_f.as_ref().map_or(true, |f| &m.level == f);
            let matches_source = source_f.as_ref().map_or(true, |f| &m.source == f);
            matches_level && matches_source && (search.is_empty() || m.message.to_lowercase().contains(&search))
        }).collect::<Vec<_>>()
    });

    // --- AUTO-SCROLL EFFECT ---
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

    // --- DRAG EVENT HANDLERS ---
    let on_mouse_down = move |ev: MouseEvent| {
        if !signals.drag_to_scroll.get() { return; }
        if let Some(el) = chat_container_ref.get() {
            set_is_dragging.set(true);
            set_start_y.set(ev.client_y());
            set_saved_scroll_top.set(el.scroll_top());

            // Instantly clear any text selection that might have accidentally started
            if let Some(window) = web_sys::window() {
                if let Ok(Some(selection)) = window.get_selection() {
                    let _ = selection.remove_all_ranges();
                }
            }
        }
    };

    let on_mouse_move = move |ev: MouseEvent| {
        if is_dragging.get() && signals.drag_to_scroll.get() {
            // STOP the browser's native text selection and boundary-scroll physics!
            ev.prevent_default();

            if let Some(el) = chat_container_ref.get() {
                let dy = ev.client_y() - start_y.get();
                el.set_scroll_top(saved_scroll_top.get() - dy);
            }
        }
    };

    let on_mouse_up_or_leave = move |_| {
        set_is_dragging.set(false);
    };

    view! {
        <div class="relative flex-1 min-h-0 flex flex-col transition-colors duration-300">
            <div
                class=move || {
                    let base = "flex-1 overflow-y-auto custom-scrollbar p-2 min-h-0";

                    // If drag-to-scroll is off, return normal classes
                    if !signals.drag_to_scroll.get() {
                        return base.to_string();
                    }

                    // If drag-to-scroll is ON, ALWAYS apply 'select-none'
                    // so the browser never attempts to highlight text
                    if is_dragging.get() {
                        format!("{} cursor-grabbing select-none", base)
                    } else {
                        format!("{} cursor-grab select-none", base)
                    }
                }
                style="overflow-anchor: auto;"
                node_ref=chat_container_ref

                on:mousedown=on_mouse_down
                on:mousemove=on_mouse_move
                on:mouseup=on_mouse_up_or_leave
                on:mouseleave=on_mouse_up_or_leave

                on:scroll=move |ev| {
                    let el = event_target::<HtmlDivElement>(&ev);
                    let scroll_top = el.scroll_top();
                    let at_bottom = el.scroll_height() - scroll_top - el.client_height() < 15;

                    // --- LOAD OLDER MESSAGES IF SCROLLED TO TOP ---
                    if scroll_top < 50 {
                        set_display_limit.update(|limit| *limit += 50);
                    }

                    if signals.active_tab.get_untracked() == "시스템" {
                        signals.set_system_at_bottom.set(at_bottom);
                    } else {
                        signals.set_is_at_bottom.set(at_bottom);
                        if at_bottom { signals.set_unread_count.set(0); }
                    }
                }
            >
                // --- SCROLLABLE CONTENT ---
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
                            let level_badge = level.clone();
                            let level_filter = level.clone();
                            let level_match = level.clone();
                            let source = sig.get().source.clone();
                            let source_badge = sig.get().source.clone();
                            view! {
                                <div class="chat chat-start opacity-90 mb-1">
                                    <div class="chat-bubble chat-bubble-xs border border-base-content/5 bg-base-300 min-h-0 text-[11px] leading-tight text-base-content">

                                        // --- LEVEL BADGE (Clickable) ---
                                        <span class=move || format!("cursor-pointer hover:brightness-125 font-black mr-1 transition-all {}",
                                                match level_badge.as_str() {
                                                    "error" => "text-error",
                                                    "warning" | "warn" => "text-warning",
                                                    "success" => "text-success",
                                                    "trace" => "text-base-content/40", // Make trace very faded
                                                    "debug" => "text-base-content/70", // Make debug slightly faded
                                                    _ => "text-info" // Default info color
                                                }
                                            )
                                            on:click=move |_| signals.set_system_level_filter.set(Some(level_filter.clone()))
                                        >
                                            "[" {move || level.clone().to_uppercase()} "]"
                                        </span>

                                        // --- SOURCE BADGE (Clickable) ---
                                        <span class="cursor-pointer hover:brightness-125 font-black mr-2 text-base-content/50 transition-all"
                                            on:click=move |_| signals.set_system_source_filter.set(Some(source_badge.clone()))
                                        >
                                            "[" {move || source.clone().to_uppercase()} "]"
                                        </span>

                                        // --- MESSAGE TEXT ---
                                        <span class=move || match level_match.to_lowercase().as_str() {
                                            "error" => "text-error",
                                            "warning" | "warn" => "text-warning",
                                            "success" => "text-success",
                                            "trace" => "text-base-content/50", // Dimmer text for trace
                                            _ => "text-base-content/90"
                                        }>
                                            {move || sig.get().message.clone()}
                                        </span>

                                        // --- TIMESTAMP ---
                                        <span class="ml-2 opacity-40 text-[9px] text-base-content/50">{move || format_time(sig.get().timestamp)}</span>
                                    </div>
                                </div>
                            }
                        }}
                    />
                </Show>
            </div>

            // --- OVERLAY: ACTIVE SEARCH / LOG FILTER TOAST ---
            // CHANGED: Expanded to show up when ANY filter is active (Level, Source, or General Search)
            <Show when=move || !signals.search_term.get().is_empty() || signals.system_level_filter.get().is_some() || signals.system_source_filter.get().is_some()>
                <div class="absolute top-4 left-1/2 -translate-x-1/2 z-50 animate-in slide-in-from-top-2 duration-200">
                    <div class="badge badge-success badge-lg gap-2 shadow-2xl font-black p-4 border border-white/20 text-success-content backdrop-blur-md bg-success/90">
                        <span class="opacity-70 text-[10px] uppercase tracking-widest">"🔍 필터링:"</span>

                        <span class="text-sm">
                            {move || {
                                let mut filters = Vec::new();
                                if let Some(l) = signals.system_level_filter.get() { filters.push(l.to_uppercase()); }
                                if let Some(s) = signals.system_source_filter.get() { filters.push(s.to_uppercase()); }

                                let st = signals.search_term.get();
                                if !st.is_empty() { filters.push(st); }

                                filters.join(" + ")
                            }}
                        </span>

                        <button class="btn btn-ghost btn-xs btn-circle ml-1 hover:bg-black/20 text-current"
                            on:click=move |_| {
                                // Clear ALL filters at once
                                signals.set_search_term.set("".to_string());
                                signals.set_system_level_filter.set(None);
                                signals.set_system_source_filter.set(None);
                            }>
                            "✕"
                        </button>
                    </div>
                </div>
            </Show>

            // --- OVERLAY: NEW MESSAGE TOAST ---
            <Show when=move || signals.unread_count.get().gt(&0) && !signals.is_at_bottom.get()>
                <div class="absolute bottom-6 left-1/2 -translate-x-1/2 z-50">
                    <button class="btn btn-success btn-sm shadow-2xl gap-2 animate-bounce"
                        on:click=move |_| {
                            if let Some(el) = chat_container_ref.get() {
                                el.set_scroll_top(el.scroll_height());
                                signals.set_is_at_bottom.set(true);
                                signals.set_unread_count.set(0);
                            }
                        }>
                        <span class="badge badge-neutral badge-sm">{move || signals.unread_count.get()}</span>
                        "새로운 메시지"
                    </button>
                </div>
            </Show>

            // --- OVERLAY: SCROLL LOCK TOAST ---
            <Show when=move || signals.active_tab.get() == "시스템" && !signals.is_system_at_bottom.get()>
                <div class="absolute bottom-6 left-1/2 -translate-x-1/2 z-50">
                    <button class="btn btn-warning btn-sm opacity-90 shadow-2xl"
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