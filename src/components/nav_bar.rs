use crate::chat_view::Tab;
use crate::components::icons::{self, icon};
use crate::readability::nav_bar_bg;
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
                let next_tab = Tab::switch_from(&signals.config.active_tab.get_untracked());

                signals
                    .config
                    .set_active_tab
                    .set(next_tab.label().to_string());
                signals.chat.set_unread_count.set(0);

                let filters = signals.config.custom_tab_filters.get_untracked();
                signals
                    .chat
                    .set_unread_counts
                    .update(|counts| next_tab.clear_unread(counts, &filters));

                signals.chat.set_is_at_bottom.set(true);
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
            class=move || format!("relative z-50 flex flex-nowrap items-center justify-between gap-x-2 px-2 py-1.5 border-b border-base-content/5 min-h-[44px] select-none transition-all duration-300 overflow-visible {}", if signals.config.compact_mode.get() { "!absolute top-0 inset-x-0 !h-8 !min-h-0 !py-0 opacity-0 hover:opacity-100 focus-within:opacity-100 bg-base-300/95 backdrop-blur-md shadow-lg transition-opacity duration-200" } else { nav_bar_bg(signals.config.overlay_opacity.get()) })
            data-tauri-drag-region
        >
            // --- LEFT: DaisyUI Tabs ---
            <div class="flex items-center gap-0.5 bg-base-content/5 p-0.5 rounded-lg min-w-0">
                {move || {
                    Tab::nav(signals.config.debug_mode.get()).into_iter().map(|tab| {
                        let full = tab.label();
                        let db_key = tab.key();
                                                let has_archive_setting = tab.has_archive_setting();
                        let db_key_click = db_key.to_string();
                        let db_key_drop = db_key.to_string();
                        let is_active = move || signals.config.active_tab.get() == full;

                        let unread = Memo::new(move |_| match tab {
                            Tab::Channel(_) | Tab::System => {
                                *signals.chat.unread_counts.get().get(db_key).unwrap_or(&0)
                            }
                            Tab::All | Tab::Custom => 0,
                        });

                                                let dot = tab.dot_class();

                        view! {
                            // REMOVED dropdown classes, replaced with standard relative flex
                            <div class="relative flex items-center h-full">

                                // 1. THE TAB BUTTON
                                <button
                                    title=full
                                    class=move || format!(
                                        "flex items-center gap-1.5 h-7 px-2.5 rounded-md text-xs whitespace-nowrap transition-colors {}",
                                        if is_active() { "bg-base-100 shadow-sm font-bold text-base-content" } else { "font-medium text-base-content/60 hover:text-base-content hover:bg-base-content/5" }
                                    )
                                    on:click=move |_| {
                                        signals.config.set_active_tab.set(full.to_string());
                                        signals.chat.set_unread_count.set(0);
                                        let filters = signals.config.custom_tab_filters.get_untracked();
                                        signals.chat.set_unread_counts.update(|counts| tab.clear_unread(counts, &filters));
                                        signals.chat.set_is_at_bottom.set(true);
                                        signals.chat.set_is_system_at_bottom.set(true);
                                        actions.save_config.dispatch(());
                                    }
                                    on:contextmenu=move |ev| {
                                        ev.prevent_default();
                                        if !matches!(tab, Tab::System | Tab::All) {
                                            set_context_menu_open.set(Some(db_key_click.clone()));
                                        }
                                    }
                                >
                                    <span class=format!("size-2 rounded-full shrink-0 {dot}")></span>
                                    <span class=move || if is_active() { "" } else if signals.config.compact_mode.get() { "hidden" } else { "hidden min-[600px]:inline" }>{full}</span>
                                    <Show when={move || unread.get() > 0}>
                                        <span class="min-w-4 h-4 px-1 rounded-full bg-error text-error-content text-[10px] font-bold leading-4 text-center">
                                            {move || if unread.get() > 99 { "99+".to_string() } else { unread.get().to_string() }}
                                        </span>
                                    </Show>
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
                                                                checked=move || signals.config.custom_tab_filters.get().contains(&ch_clone)
                                                                on:change=move |ev| {
                                                                    let checked = event_target_checked(&ev);
                                                                    signals.config.set_custom_tab_filters.update(|f| {
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
                                                    prop:value=move || signals.config.tab_limits.get().get(db_key).copied().unwrap_or(if tab == Tab::Channel(Channel::World) { 200 } else { 1000 }).to_string()
                                                    on:change=move |ev| {
                                                        let val = event_target_value(&ev).parse::<usize>().unwrap_or(500);
                                                        signals.config.set_tab_limits.update(|map| { map.insert(db_key.to_string(), val); });
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
                                                        prop:checked=move || !signals.config.archive_ignored_channels.get().contains(&db_key.to_string())
                                                        on:change=move |ev| {
                                                            let is_checked = event_target_checked(&ev);
                                                            signals.config.set_archive_ignored_channels.update(|list| {
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
                        prop:value=move || signals.chat.search_term.get()
                        on:input=move |ev| signals.chat.set_search_term.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Escape" {
                                set_is_search_open.set(false);
                            }
                        }
                    />
                    <button class="btn btn-ghost btn-xs btn-circle text-base-content/50 hover:text-error"
                        on:click=move |_| {
                            signals.chat.set_search_term.set("".to_string());
                            set_is_search_open.set(false);
                        }>
                        "✕"
                    </button>
                </div>
            </div>

            // --- compact mode: the bar shows on hover only -- pin + leave compact ---
            <Show when=move || signals.config.compact_mode.get()>
                <div class="flex items-center gap-0.5 ml-auto shrink-0" data-tauri-no-drag>
                    <button class="btn btn-ghost btn-xs btn-square" title="항상 위에 표시"
                        class:text-success=move || signals.config.always_on_top.get()
                        class:text-base-content=move || !signals.config.always_on_top.get()
                        on:click=move |_| {
                            let new_state = !signals.config.always_on_top.get();
                            signals.config.set_always_on_top.set(new_state);
                            spawn_local(async move {
                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({"onTop": new_state})).unwrap();
                                let _ = invoke("set_always_on_top", args).await;
                            });
                            actions.save_config.dispatch(());
                        }>
                        <span class=move || if signals.config.always_on_top.get() { "block" } else { "block rotate-45 opacity-50" }>{icon(icons::PIN, "size-3.5")}</span>
                    </button>
                    <button class="btn btn-ghost btn-xs btn-square text-base-content/60 hover:text-base-content" title="컴팩트 모드 끄기"
                        on:click=move |_| {
                            signals.config.set_compact_mode.set(false);
                            actions.save_config.dispatch(());
                        }>
                        {icon(icons::EXPAND, "size-3.5")}
                    </button>
                </div>
            </Show>
            // --- RIGHT: favorites, then the folded tools (normal mode) ---
            <div class:hidden=move || signals.config.compact_mode.get() class="flex items-center gap-1 ml-auto shrink-0" data-tauri-no-drag>
                <div class="tooltip tooltip-bottom" data-tip="자주 쓰는 메시지">
                    <button class="btn btn-ghost btn-sm btn-square text-base-content/60 hover:text-base-content"
                        on:click=move |_| signals.ui.set_show_favorites.set(true)>
                        {icon(icons::STAR, "size-4")}
                    </button>
                </div>

                // The tools are always folded behind one button: they open
                // below the bar, so they never cover the tabs.
                <div class="relative"
                    on:mouseenter=move |_| set_is_controls_open.set(true)
                    on:mouseleave=move |_| set_is_controls_open.set(false)
                >
                    <button
                        node_ref=folder_btn_ref
                        class="btn btn-ghost btn-sm btn-square"
                        class:text-success=move || is_controls_open.get()
                        title="도구"
                        on:click=move |_| set_is_controls_open.set(true)
                    >
                        {icon(icons::MORE, "size-4")}
                    </button>

                    <div
                        node_ref=controls_container_ref
                        class=move || if is_controls_open.get() {
                            "absolute right-0 top-full pt-1 z-[55]"
                        } else {
                            "hidden"
                        }
                    >
                        <div class="flex items-center gap-1 bg-base-300 p-1 rounded-lg shadow-2xl border border-white/10 animate-in fade-in duration-150">
                            <div class="tooltip tooltip-bottom" data-tip="Search (Ctrl+F)">
                                <button
                                    node_ref=search_btn_ref
                                    class="btn btn-ghost btn-sm btn-square text-base-content/60 hover:text-base-content"
                                    class:text-success=move || !signals.chat.search_term.get().is_empty()
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
                                    {icon(icons::SEARCH, "size-4")}
                                </button>
                            </div>

                            <div class="tooltip tooltip-bottom" data-tip="Always on Top">
                                <button class="btn btn-sm btn-square"
                                    class:btn-success=move || signals.config.always_on_top.get()
                                    class:btn-soft=move || signals.config.always_on_top.get()
                                    class:btn-ghost=move || !signals.config.always_on_top.get()
                                    class:text-base-content=move || !signals.config.always_on_top.get()
                                    on:click=move |_| {
                                        let new_state = !signals.config.always_on_top.get();
                                        signals.config.set_always_on_top.set(new_state);
                                        spawn_local(async move {
                                            let args = serde_wasm_bindgen::to_value(&serde_json::json!({"onTop": new_state})).unwrap();
                                            let _ = invoke("set_always_on_top", args).await;
                                        });
                                        actions.save_config.dispatch(());
                                    }>
                                    <span class=move || if signals.config.always_on_top.get() { "block" } else { "block rotate-45 opacity-60" }>{icon(icons::PIN, "size-4")}</span>
                                </button>
                            </div>

                            <div class="relative group flex items-center justify-center">
                                <div class="tooltip tooltip-bottom" data-tip="Background Opacity">
                                    <button class="btn btn-ghost btn-sm btn-square text-base-content/60 hover:text-base-content">
                                        {icon(icons::CONTRAST, "size-4")}
                                    </button>
                                </div>
                                <div class="absolute top-full right-1/2 translate-x-1/2 pt-1.5 z-50 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-all duration-200">
                                    <div class="bg-base-300 border border-base-content/10 rounded-lg shadow-xl p-3 w-32 flex flex-col gap-2 items-center cursor-default">
                                        <span class="text-[9px] font-black text-success uppercase tracking-widest opacity-80">
                                            {move || format!("투명도: {:.0}%", signals.config.overlay_opacity.get() * 100.0)}
                                        </span>
                                        <input type="range" min="0.0" max="1.0" step="0.05"
                                            class="range range-xs range-success w-full"
                                            prop:value=move || signals.config.overlay_opacity.get().to_string()
                                            on:input=move |ev| {
                                                let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                                signals.config.set_overlay_opacity.set(val);
                                            }
                                            on:change=move |ev| {
                                                let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                                signals.config.set_overlay_opacity.set(val);
                                                actions.save_config.dispatch(());
                                            }
                                        />
                                    </div>
                                </div>
                            </div>

                            <div class="tooltip tooltip-bottom" data-tip="Clear History">
                                <button class="btn btn-ghost btn-sm btn-square text-base-content/60 hover:text-base-content hover:!text-error"
                                    on:click=move |_| { actions.clear_history.dispatch(()); }>
                                    {icon(icons::TRASH, "size-4")}
                                </button>
                            </div>

                            <div class="tooltip tooltip-bottom" data-tip="Settings">
                                <button class="btn btn-ghost btn-sm btn-square text-base-content/60 hover:text-base-content" aria-label="Settings" on:click=move |_| signals.ui.set_show_settings.set(true)>
                                    {icon(icons::GEAR, "size-4")}
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </nav>
    }
}
