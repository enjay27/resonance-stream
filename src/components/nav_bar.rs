use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;
use leptos::html::{Input, Div, Button}; // ADDED: Div and Button for our new refs
use leptos::ev::{keydown, click};
use wasm_bindgen::closure::Closure;
// ADDED: click event
use web_sys::Node; // ADDED: Node to verify click targets

#[component]
pub fn NavBar() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    let (is_search_open, set_is_search_open) = signal(false);
    let (is_controls_open, set_is_controls_open) = signal(false);

    // --- NODE REFERENCES ---
    let search_input_ref = create_node_ref::<Input>();
    let search_container_ref = create_node_ref::<Div>(); // The absolute popup box
    let search_btn_ref = create_node_ref::<Button>(); // The magnifier toggle button
    let controls_container_ref = create_node_ref::<Div>();
    let folder_btn_ref = create_node_ref::<Button>();

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

    Effect::new(move |_| {
        spawn_local(async move {
            let closure = Closure::wrap(Box::new(move |_: JsValue| {
                // This triggers ANYTIME the global shortcut is pressed, even during gameplay!
                let sequence = vec!["커스텀", "월드", "길드", "파티", "로컬"];
                let current = signals.active_tab.get_untracked();

                let next_tab = if let Some(idx) = sequence.iter().position(|&x| x == current) {
                    sequence[(idx + 1) % sequence.len()].to_string()
                } else {
                    "커스텀".to_string()
                };

                signals.set_active_tab.set(next_tab.clone());
                signals.set_unread_count.set(0);

                signals.set_unread_counts.update(|counts| {
                    match next_tab.as_str() {
                        "커스텀" => {
                            let filters = signals.custom_filters.get_untracked();
                            for f in filters { counts.remove(&f); }
                        },
                        "월드" => { counts.remove("WORLD"); },
                        "길드" => { counts.remove("GUILD"); },
                        "파티" => { counts.remove("PARTY"); },
                        "로컬" => { counts.remove("LOCAL"); },
                        _ => {}
                    }
                });

                signals.set_is_at_bottom.set(true);
                actions.save_config.dispatch(());
            }) as Box<dyn FnMut(JsValue)>);

            let _ = crate::tauri_bridge::listen("global-tab-switch", &closure).await;
            closure.forget();
        });
    });

    // ==========================================
    // CLICK-OUTSIDE TO CLOSE (Search & Controls)
    // ==========================================
    window_event_listener(click, move |ev| {
        let target = event_target::<Node>(&ev);

        if is_search_open.get_untracked() {
            let container = search_container_ref.get();
            let btn = search_btn_ref.get();
            let clicked_inside = container.map(|c| c.contains(Some(&target))).unwrap_or(false);
            let clicked_btn = btn.map(|b| b.contains(Some(&target))).unwrap_or(false);

            if !clicked_inside && !clicked_btn {
                set_is_search_open.set(false);
            }
        }

        if is_controls_open.get_untracked() {
            let container = controls_container_ref.get();
            let btn = folder_btn_ref.get();
            let clicked_inside = container.map(|c| c.contains(Some(&target))).unwrap_or(false);
            let clicked_btn = btn.map(|b| b.contains(Some(&target))).unwrap_or(false);

            if !clicked_inside && !clicked_btn {
                set_is_controls_open.set(false);
            }
        }
    });

    view! {
        <nav
            class="relative z-50 flex flex-nowrap items-center justify-between gap-x-2 px-2 py-1 bg-base-content/5 border-b border-base-content/5 min-h-[40px] select-none transition-all duration-300 overflow-visible"
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
                        let f_unread = full.to_string();
                        let is_custom = full == "커스텀";
                        let is_active = move || signals.active_tab.get() == t_full;

                        let unread = Memo::new(move |_| {
                            let counts = signals.unread_counts.get();
                            match f_unread.as_str() {
                                "전체" => 0,
                                "커스텀" => 0,
                                "월드" => *counts.get("WORLD").unwrap_or(&0),
                                "길드" => *counts.get("GUILD").unwrap_or(&0),
                                "파티" => *counts.get("PARTY").unwrap_or(&0),
                                "로컬" => *counts.get("LOCAL").unwrap_or(&0),
                                "시스템" => *counts.get("SYSTEM").unwrap_or(&0),
                                _ => 0,
                            }
                        });

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
                            <div class="relative group flex items-center h-full">
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
                                        signals.set_unread_counts.update(|counts| {
                                            match t_click.as_str() {
                                                "전체" => counts.clear(),
                                                "커스텀" => {
                                                    let filters = signals.custom_filters.get_untracked();
                                                    for f in filters { counts.remove(&f); }
                                                },
                                                "월드" => { counts.remove("WORLD"); },
                                                "길드" => { counts.remove("GUILD"); },
                                                "파티" => { counts.remove("PARTY"); },
                                                "로컬" => { counts.remove("LOCAL"); },
                                                "시스템" => { counts.remove("SYSTEM"); },
                                                _ => {}
                                            }
                                        });
                                        signals.set_is_at_bottom.set(true);
                                        signals.set_system_at_bottom.set(true);
                                        actions.save_config.dispatch(());
                                    }
                                >
                                    // Text only (Shows when narrower than 460px)
                                    <span class="min-[460px]:hidden flex items-center">
                                        {full}
                                        <Show when={move || unread.get() > 0}>
                                            <span class="badge badge-error min-w-[14px] h-[14px] px-1 ml-0.5 text-white text-[9px] font-black border-none shadow-sm shadow-error/30 animate-in zoom-in duration-200">
                                                {move || if unread.get() > 9 { "9+".to_string() } else { unread.get().to_string() }}
                                            </span>
                                        </Show>
                                    </span>

                                    // Text + Emoji (Shows when wider than 400px)
                                    <span class="hidden min-[460px]:flex items-center">
                                        {full} " " {icon}
                                        <Show when={move || unread.get() > 0}>
                                            <span class="badge badge-error min-w-[14px] h-[14px] px-1 ml-1 text-white text-[9px] font-black border-none shadow-sm shadow-error/30 animate-in zoom-in duration-200">
                                                {move || if unread.get() > 9 { "9+".to_string() } else { unread.get().to_string() }}
                                            </span>
                                        </Show>
                                    </span>
                                </button>

                                // NEW: Dropdown Menu that appears when hovering the '커스텀' tab
                                <Show when=move || is_custom>
                                    <div class="absolute top-full left-0 pt-2 z-50 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-all duration-200">
                                        <div class="bg-base-300 border border-base-content/10 rounded-lg shadow-2xl p-2 w-36 flex flex-col gap-1">
                                            <span class="text-[9px] font-black text-success uppercase tracking-widest px-1 mb-1 opacity-80">"필터 설정"</span>

                                            {vec!["WORLD", "GUILD", "PARTY", "LOCAL"].into_iter().map(|channel| {
                                                let ch = channel.to_string();
                                                let ch_clone = ch.clone();
                                                view! {
                                                    <label class="label cursor-pointer flex justify-between px-1.5 py-1 hover:bg-base-content/10 rounded">
                                                        <span class="label-text text-[10px] font-bold">{channel}</span>
                                                        <input type="checkbox" class="checkbox checkbox-xs checkbox-success"
                                                            checked=move || signals.custom_filters.get().contains(&ch_clone)
                                                            on:change=move |ev| {
                                                                let checked = event_target_checked(&ev);
                                                                signals.set_custom_filters.update(|f| {
                                                                    if checked { f.push(ch.clone()); }
                                                                    else { f.retain(|x| x != &ch); }
                                                                });
                                                                actions.save_config.dispatch(());
                                                            }
                                                        />
                                                    </label>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </div>
                                </Show>
                            </div>
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
            <div class="flex items-center gap-1 ml-auto" data-tauri-no-drag
                on:mouseenter=move |_| set_is_controls_open.set(true)
                on:mouseleave=move |_| set_is_controls_open.set(false)
            >

                // Triangle Toggle Button (Folds when width < 650px)
                <button
                    node_ref=folder_btn_ref
                    class="btn btn-ghost btn-xs text-lg min-[675px]:hidden z-[60]"
                    class:text-success=move || is_controls_open.get()
                    on:click=move |_| set_is_controls_open.update(|b| *b = !*b)
                >
                    {move || if is_controls_open.get() { "▶" } else { "◀" }}
                </button>

                // Control Icons Wrapper
                <div
                    node_ref=controls_container_ref
                    class=move || format!(
                        "items-center gap-1 min-[675px]:flex min-[675px]:static min-[675px]:bg-transparent min-[675px]:shadow-none min-[675px]:p-0 min-[675px]:border-none transition-all duration-200 z-[55] {}",
                        if is_controls_open.get() {
                            // Expanded Overlapping View
                            "absolute right-10 top-1.5 flex bg-base-300 p-1 rounded-lg shadow-2xl border border-white/10 animate-in slide-in-from-right-2"
                        } else {
                            // Hidden View
                            "hidden"
                        }
                    )
                >
                    // The Magnifier Button
                    <div class="tooltip tooltip-bottom" data-tip="Search (Ctrl+F)">
                        <button
                            node_ref=search_btn_ref
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

                    // Background Opacity Control
                    <div class="relative group flex items-center justify-center">
                        <div class="tooltip tooltip-bottom" data-tip="Background Opacity">
                            <button class="btn btn-ghost btn-xs text-lg">
                                "🌗"
                            </button>
                        </div>

                        // Opacity Slider Popup on Hover
                        <div class="absolute top-full right-1/2 translate-x-1/2 pt-1.5 z-50 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-all duration-200">
                            <div class="bg-base-300 border border-base-content/10 rounded-lg shadow-xl p-3 w-32 flex flex-col gap-2 items-center cursor-default">
                                <span class="text-[9px] font-black text-success uppercase tracking-widest opacity-80">
                                    {move || format!("투명도: {:.0}%", signals.opacity.get() * 100.0)}
                                </span>
                                <input type="range" min="0.0" max="1.0" step="0.05"
                                    class="range range-xs range-success w-full"
                                    prop:value=move || signals.opacity.get().to_string()
                                    on:input=move |ev| {
                                        let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                        signals.set_opacity.set(val);
                                    }
                                    on:change=move |ev| {
                                        let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                        signals.set_opacity.set(val);
                                        actions.save_config.dispatch(());
                                    }
                                />
                            </div>
                        </div>
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
                        </button>
                    </div>
                </div>
            </div>
        </nav>
    }
}