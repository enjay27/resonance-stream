use crate::chat_view::Tab;
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use crate::ui_types::Channel;
use leptos::ev::{click, keydown};
use leptos::html::{Button, Div, Input};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsValue;
use web_sys::Node;

#[component]
pub fn NavBar() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    let (is_search_open, set_is_search_open) = signal(false);
    let (is_controls_open, set_is_controls_open) = signal(false);
    let (context_menu_open, set_context_menu_open) = signal(None::<String>);

    // --- NODE REFERENCES ---
    let search_input_ref = NodeRef::<Input>::new();
    let search_container_ref = NodeRef::<Div>::new();
    let search_btn_ref = NodeRef::<Button>::new();
    let controls_container_ref = NodeRef::<Div>::new();
    let folder_btn_ref = NodeRef::<Button>::new();

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
                let next_tab = Tab::switch_from(&signals.active_tab.get_untracked());

                signals.set_active_tab.set(next_tab.label().to_string());
                signals.set_unread_count.set(0);

                let filters = signals.custom_filters.get_untracked();
                signals
                    .set_unread_counts
                    .update(|counts| next_tab.clear_unread(counts, &filters));

                signals.set_is_at_bottom.set(true);
                actions.save_config.dispatch(());
            }) as Box<dyn FnMut(JsValue)>);

            let _ = crate::tauri_bridge::listen("global-tab-switch", &closure).await;
            closure.forget();
        });
    });

    // ==========================================
    // CLICK-OUTSIDE TO CLOSE LISTENERS
    // ==========================================
    window_event_listener(click, move |ev| {
        let target = event_target::<Node>(&ev);

        // 1. Close Right-Click Menu
        if context_menu_open.get_untracked().is_some() {
            set_context_menu_open.set(None);
        }

        // 2. Close Search
        if is_search_open.get_untracked() {
            let container = search_container_ref.get();
            let btn = search_btn_ref.get();
            let clicked_inside = container
                .map(|c| c.contains(Some(&target)))
                .unwrap_or(false);
            let clicked_btn = btn.map(|b| b.contains(Some(&target))).unwrap_or(false);
            if !clicked_inside && !clicked_btn {
                set_is_search_open.set(false);
            }
        }

        // 3. Close Controls
        if is_controls_open.get_untracked() {
            let container = controls_container_ref.get();
            let btn = folder_btn_ref.get();
            let clicked_inside = container
                .map(|c| c.contains(Some(&target)))
                .unwrap_or(false);
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
                    Tab::nav(signals.debug_mode.get()).into_iter().map(|tab| {
                        let full = tab.label();
                        let db_key = tab.key();
                        let icon = tab.icon();
                        let has_archive_setting = tab.has_archive_setting();
                        let db_key_click = db_key.to_string();
                        let db_key_drop = db_key.to_string();
                        let is_active = move || signals.active_tab.get() == full;

                        let unread = Memo::new(move |_| match tab {
                            Tab::Channel(_) | Tab::System => {
                                *signals.unread_counts.get().get(db_key).unwrap_or(&0)
                            }
                            Tab::All | Tab::Custom => 0,
                        });

                        let (text_color, border_color) = tab.colors();

                        view! {
                            // REMOVED dropdown classes, replaced with standard relative flex
                            <div class="relative flex items-center h-full">

                                // 1. THE TAB BUTTON
                                <button
                                    class=move || format!(
                                        "join-item btn btn-xs h-7 px-3 rounded-none transition-all font-black border-0 border-b-[3px] !overflow-visible flex flex-nowrap items-center {} {}",
                                        text_color,
                                        if is_active() {
                                            format!("font-black {} {} bg-white/5 opacity-100", text_color, border_color)
                                        } else {
                                            format!("font-bold hover:font-black {} border-transparent bg-transparent opacity-70 hover:opacity-100", text_color)
                                        }
                                    )
                                    on:click=move |_| {
                                        signals.set_active_tab.set(full.to_string());
                                        signals.set_unread_count.set(0);
                                        let filters = signals.custom_filters.get_untracked();
                                        signals.set_unread_counts.update(|counts| tab.clear_unread(counts, &filters));
                                        signals.set_is_at_bottom.set(true);
                                        signals.set_system_at_bottom.set(true);
                                        actions.save_config.dispatch(());
                                    }
                                    on:contextmenu=move |ev| {
                                        ev.prevent_default();
                                        if !matches!(tab, Tab::System | Tab::All) {
                                            set_context_menu_open.set(Some(db_key_click.clone()));
                                        }
                                    }
                                >
                                    // Text only (Shows when narrower than 460px)
                                    <span class="min-[460px]:hidden flex items-center relative gap-1">
                                        {full}
                                        <Show when={move || unread.get() > 0}>
                                            <span class="absolute -top-1.5 -right-2.5 badge badge-error min-w-[14px] h-[14px] px-1 text-white text-[9px] font-black border-none shadow-sm shadow-error/30 animate-in zoom-in duration-200 z-10">
                                                {move || if unread.get() > 9 { "9+".to_string() } else { unread.get().to_string() }}
                                            </span>
                                        </Show>
                                    </span>

                                    // Text + Emoji (Shows when wider than 460px)
                                    <span class="hidden min-[460px]:flex items-center relative gap-1">
                                        {full} " " {icon}
                                        <Show when={move || unread.get() > 0}>
                                            <span class="absolute -top-1.5 -right-2.5 badge badge-error min-w-[14px] h-[14px] px-1 text-white text-[9px] font-black border-none shadow-sm shadow-error/30 animate-in zoom-in duration-200 z-10">
                                                {move || if unread.get() > 9 { "9+".to_string() } else { unread.get().to_string() }}
                                            </span>
                                        </Show>
                                    </span>
                                </button>

                                // 2. THE SETTINGS DROPDOWN MENU
                                <Show when=move || context_menu_open.get() == Some(db_key_drop.clone())>
                                    <ul
                                        class="absolute top-full left-0 z-[100] menu p-3 shadow-2xl bg-base-300 rounded-box w-64 border border-base-content/10 mt-1 space-y-3 cursor-default animate-in fade-in slide-in-from-top-2 duration-100"
                                        on:click=move |ev| ev.stop_propagation() // Prevent clicks from closing the menu
                                    >
                                        <h3 class="text-[11px] font-black opacity-50 border-b border-base-content/10 pb-1.5 uppercase tracking-wider">
                                            {full} " 채널 설정"
                                        </h3>

                                        // --- ONLY FOR CUSTOM TAB: Channel filter selection ---
                                        <Show when=move || tab == Tab::Custom>
                                            <div class="space-y-1 mb-2">
                                                <span class="text-[10px] font-bold text-success">"표시할 채널 선택:"</span>
                                                {Channel::ALL.into_iter().map(|channel| {
                                                    let ch = channel.as_str().to_string();
                                                    let ch_clone = ch.clone();
                                                    view! {
                                                        <label class="label cursor-pointer flex justify-between px-1.5 py-0 hover:bg-base-content/10 rounded">
                                                            <span class="label-text text-[10px] font-bold">{channel.as_str()}</span>
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
                                        </Show>

                                        // --- MAX RAM LIMIT INPUT (Hidden for Custom since it aggregates dynamically) ---
                                        <Show when=move || tab != Tab::Custom>
                                            <div class="flex items-center justify-between">
                                                <span class="text-xs font-bold text-base-content/80">"최대 메시지 유지:"</span>
                                                <input type="number" class="input input-xs input-bordered w-16 text-right font-mono bg-base-200 focus:border-success"
                                                    prop:value=move || signals.tab_limits.get().get(db_key).copied().unwrap_or(if tab == Tab::Channel(Channel::World) { 200 } else { 1000 }).to_string()
                                                    on:change=move |ev| {
                                                        let val = event_target_value(&ev).parse::<usize>().unwrap_or(500);
                                                        signals.set_tab_limits.update(|map| { map.insert(db_key.to_string(), val); });
                                                        actions.save_config.dispatch(());
                                                    }
                                                />
                                            </div>
                                        </Show>

                                        // --- DISK LOGGING SAVE TOGGLE ---
                                        <Show when=move || has_archive_setting>
                                            <div class="form-control mt-1 pt-2 border-t border-base-content/5">
                                                <label class="label cursor-pointer p-0 hover:bg-transparent">
                                                    <span class="label-text text-xs font-bold text-success">"디스크 자동 저장"</span>
                                                    <input type="checkbox" class="checkbox checkbox-xs checkbox-success"
                                                        prop:checked=move || !signals.archive_ignored_channels.get().contains(&db_key.to_string())
                                                        on:change=move |ev| {
                                                            let is_checked = event_target_checked(&ev);
                                                            signals.set_archive_ignored_channels.update(|list| {
                                                                if is_checked {
                                                                    // Enable saving = Remove from the ignored list
                                                                    list.retain(|c| c != db_key);
                                                                } else {
                                                                    // Disable saving = Add to the ignored list
                                                                    if !list.contains(&db_key.to_string()) { list.push(db_key.to_string()); }
                                                                }
                                                            });
                                                            actions.save_config.dispatch(());
                                                        }
                                                    />
                                                </label>
                                            </div>
                                        </Show>
                                    </ul>
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
                node_ref=search_container_ref
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
                <button
                    node_ref=folder_btn_ref
                    class="btn btn-ghost btn-xs text-lg min-[675px]:hidden z-[60]"
                    class:text-success=move || is_controls_open.get()
                    on:click=move |_| set_is_controls_open.update(|b| *b = !*b)
                >
                    {move || if is_controls_open.get() { "▶" } else { "◀" }}
                </button>

                <div
                    node_ref=controls_container_ref
                    class=move || format!(
                        "items-center gap-1 min-[675px]:flex min-[675px]:static min-[675px]:bg-transparent min-[675px]:shadow-none min-[675px]:p-0 min-[675px]:border-none transition-all duration-200 z-[55] {}",
                        if is_controls_open.get() {
                            "absolute right-10 top-1.5 flex bg-base-300 p-1 rounded-lg shadow-2xl border border-white/10 animate-in slide-in-from-right-2"
                        } else {
                            "hidden"
                        }
                    )
                >
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

                    <div class="tooltip tooltip-bottom" data-tip="자주 쓰는 메시지">
                        <button class="btn btn-ghost btn-xs text-lg"
                            on:click=move |_| signals.set_show_favorites.set(true)>
                            "⭐"
                        </button>
                    </div>

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

                    <div class="relative group flex items-center justify-center">
                        <div class="tooltip tooltip-bottom" data-tip="Background Opacity">
                            <button class="btn btn-ghost btn-xs text-lg">
                                "🌗"
                            </button>
                        </div>
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

                    <div class="tooltip tooltip-bottom" data-tip="Compact Mode">
                        <button class="btn btn-ghost btn-xs text-lg"
                            on:click=move |_| {
                                let new_compact_state = !signals.compact_mode.get_untracked();
                                signals.set_compact_mode.set(new_compact_state);

                                if new_compact_state && signals.active_tab.get_untracked() != Tab::System.label() {
                                    signals.set_active_tab.set(Tab::Custom.label().to_string());
                                }
                                actions.save_config.dispatch(());
                            }>
                            {move || if signals.compact_mode.get() { "🔽" } else { "🔼" }}
                        </button>
                    </div>

                    <div class="tooltip tooltip-bottom" data-tip="Clear History">
                        <button class="btn btn-ghost btn-xs text-lg hover:text-error"
                            on:click=move |_| { actions.clear_history.dispatch(()); }>
                            "🗑️"
                        </button>
                    </div>

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
