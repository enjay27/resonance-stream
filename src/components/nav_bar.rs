use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;
use leptos::html::{Input, Div, Button}; // ADDED: Div and Button for our new refs
use leptos::ev::{keydown, click}; // ADDED: click event
use web_sys::Node; // ADDED: Node to verify click targets

#[component]
pub fn NavBar() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    let (is_search_open, set_is_search_open) = signal(false);

    // --- NODE REFERENCES ---
    let search_input_ref = create_node_ref::<Input>();
    let search_container_ref = create_node_ref::<Div>(); // The absolute popup box
    let search_btn_ref = create_node_ref::<Button>(); // The magnifier toggle button

    // ==========================================
    // GLOBAL KEYBOARD SHORTCUT (Ctrl+F)
    // ==========================================
    window_event_listener(keydown, move |ev| {
        if (ev.ctrl_key() || ev.meta_key()) && ev.key().to_lowercase() == "f" {
            ev.prevent_default();
            set_is_search_open.set(true);

            request_animation_frame(move || {
                if let Some(el) = search_input_ref.get() {
                    let _ = el.focus();
                    el.select();
                }
            });
        }
    });

    // ==========================================
    // NEW: CLICK-OUTSIDE TO CLOSE
    // ==========================================
    window_event_listener(click, move |ev| {
        // Only run this logic if the search bar is actually open
        if is_search_open.get_untracked() {
            let target = event_target::<Node>(&ev);

            let container = search_container_ref.get();
            let btn = search_btn_ref.get();

            // Check if the click target is inside the Search Box OR the Magnifier Button
            let clicked_inside = container.map(|c| c.contains(Some(&target))).unwrap_or(false);
            let clicked_btn = btn.map(|b| b.contains(Some(&target))).unwrap_or(false);

            // If they clicked somewhere else entirely, close it!
            if !clicked_inside && !clicked_btn {
                set_is_search_open.set(false);
            }
        }
    });

    view! {
        <nav
            class="relative flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5 px-2 py-1 bg-base-content/5 border-b border-base-content/5 min-h-[40px] select-none transition-all duration-300"
            data-tauri-drag-region
        >

            // --- LEFT: DaisyUI Tabs ---
            <div class="join bg-base-300/50 p-0.5 rounded-lg border border-base-content/5 flex-shrink-0">
                {move || {
                    let mut tabs = vec![
                        ("전체", "♾️"), ("커스텀", "⭐"), ("월드", "🌐"),
                        ("길드", "🛡️"), ("파티", "⚔️"), ("로컬", "📍"),
                    ];
                    if signals.debug_mode.get() { tabs.push(("시스템", "⚙️")); }

                    tabs.into_iter().map(|(full, icon)| {
                        let t_full = full.to_string();
                        let t_click = t_full.clone();
                        let is_active = move || signals.active_tab.get() == t_full;

                        let (text_color, border_color) = match full {
                            "전체" => ("text-base-content", "border-base-content"),
                            "커스텀" => ("text-success", "border-success"),
                            "월드" => ("text-purple-500", "border-purple-500"),
                            "길드" => ("text-emerald-500", "border-emerald-500"),
                            "파티" => ("text-sky-500", "border-sky-500"),
                            "로컬" => ("text-base-content opacity-70", "border-base-content opacity-70"),
                            "시스템" => ("text-warning", "border-warning"),
                            _ => ("text-base-content", "border-transparent"),
                        };

                        view! {
                            <button
                                class=move || format!(
                                    "join-item btn btn-xs h-7 px-3 rounded-none transition-all font-black border-0 border-b-[3px] {} {}",
                                    text_color,
                                    if is_active() {
                                        format!("{} bg-white/5 opacity-100", border_color)
                                    } else {
                                        "border-transparent bg-transparent opacity-40 hover:opacity-100".to_string()
                                    }
                                )
                                on:click=move |_| {
                                    signals.set_active_tab.set(t_click.clone());
                                    signals.set_unread_count.set(0);
                                    signals.set_is_at_bottom.set(true);
                                    signals.set_system_at_bottom.set(true);
                                    actions.save_config.dispatch(());
                                }
                            >
                                <span class="sm:hidden">{full}</span>
                                <span class="hidden sm:inline">{full} " " {icon}</span>
                            </button>
                        }
                    }).collect_view()
                }}
            </div>

            // ==========================================
            // CENTER: GLOBAL SEARCH PALETTE
            // ==========================================
            <div
                node_ref=search_container_ref // <-- Attached ref here
                class=move || format!(
                    "absolute left-1/2 -translate-x-1/2 top-full mt-2 p-1.5 bg-base-300 border border-base-content/10 rounded-lg shadow-2xl z-50 transition-all duration-200 origin-top {}",
                    if is_search_open.get() { "opacity-100 scale-100 pointer-events-auto" } else { "opacity-0 scale-95 pointer-events-none" }
                )
            >
                <div class="flex items-center gap-1">
                    <input type="text" placeholder="대화 검색 (Ctrl+F)..."
                        node_ref=search_input_ref
                        class="input input-xs input-bordered w-64 bg-base-200 text-xs focus:outline-none focus:border-success"
                        prop:value=move || signals.search_term.get()
                        on:input=move |ev| signals.set_search_term.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Escape" {
                                set_is_search_open.set(false);
                            }
                        }
                    />
                    <button class="btn btn-ghost btn-xs btn-circle text-base-content/50 hover:text-error"
                        on:click=move |_| {
                            signals.set_search_term.set("".to_string());
                            set_is_search_open.set(false);
                        }>
                        "✕"
                    </button>
                </div>
            </div>

            // --- RIGHT: Control Icons ---
            <div class="flex items-center gap-1 ml-auto" data-tauri-no-drag>

                // The Magnifier Button
                <div class="tooltip tooltip-bottom" data-tip="Search (Ctrl+F)">
                    <button
                        node_ref=search_btn_ref // <-- Attached ref here
                        class="btn btn-ghost btn-xs text-lg"
                        class:text-success=move || !signals.search_term.get().is_empty()
                        on:click=move |_| {
                            let new_state = !is_search_open.get_untracked();
                            set_is_search_open.set(new_state);

                            if new_state {
                                request_animation_frame(move || {
                                    if let Some(el) = search_input_ref.get() {
                                        let _ = el.focus();
                                        el.select();
                                    }
                                });
                            }
                        }
                    >
                        "🔍"
                    </button>
                </div>

                <div class="divider divider-horizontal mx-0 opacity-10"></div>

                // Always on Top
                <div class="tooltip tooltip-bottom" data-tip="Always on Top">
                    <button class="btn btn-xs"
                        class:btn-success=move || signals.is_pinned.get()
                        class:btn-ghost=move || !signals.is_pinned.get()
                        on:click=move |_| {
                            let new_state = !signals.is_pinned.get();
                            signals.set_is_pinned.set(new_state);
                            spawn_local(async move {
                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({"onTop": new_state})).unwrap();
                                let _ = invoke("set_always_on_top", args).await;
                            });
                            actions.save_config.dispatch(());
                        }>
                        <span class=move || if signals.is_pinned.get() { "rotate-45 block" } else { "block" }>"📌"</span>
                    </button>
                </div>

                // Compact Mode Toggle
                <div class="tooltip tooltip-bottom" data-tip="Compact Mode">
                    <button class="btn btn-ghost btn-xs text-lg"
                        on:click=move |_| {
                            let new_compact_state = !signals.compact_mode.get_untracked();
                            signals.set_compact_mode.set(new_compact_state);

                            if new_compact_state && signals.active_tab.get_untracked() != "시스템" {
                                signals.set_active_tab.set("커스텀".to_string());
                            }

                            actions.save_config.dispatch(());
                        }>
                        {move || if signals.compact_mode.get() { "🔽" } else { "🔼" }}
                    </button>
                </div>

                // Clear Chat History
                <div class="tooltip tooltip-bottom" data-tip="Clear History">
                    <button class="btn btn-ghost btn-xs text-lg hover:text-error"
                        on:click=move |_| { actions.clear_history.dispatch(()); }>
                        "🗑️"
                    </button>
                </div>

                // Settings & Restart Indicator
                <div class="tooltip tooltip-bottom" data-tip="Settings">
                    <button class="btn btn-ghost btn-xs relative" on:click=move |_| signals.set_show_settings.set(true)>
                        "⚙️"
                        <Show when=move || signals.restart_required.get()>
                            <span class="absolute top-0 right-0 w-2 h-2 bg-warning rounded-full animate-ping"></span>
                            <span class="absolute top-0 right-0 w-2 h-2 bg-warning rounded-full"></span>
                        </Show>
                    </button>
                </div>
            </div>
        </nav>
    }
}