use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

#[component]
pub fn NavBar() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    // --- DICTIONARY SYNC ACTION (Restored from app.rs) ---
    let sync_dict_action = Action::new_local(|_: &()| async move {
        match invoke("sync_dictionary", JsValue::NULL).await {
            Ok(_) => "최신 상태".to_string(),
            Err(_) => "동기화 실패".to_string(),
        }
    });

    let is_syncing = sync_dict_action.pending();

    view! {
        <nav class="flex items-center justify-between px-2 bg-base-200 border-b border-white/5 h-10 select-none">

            // --- LEFT: DaisyUI Tabs ---
            <div class="join bg-base-300/50 p-0.5 rounded-lg border border-white/5">
                {move || {
                    let mut tabs = vec![
                        ("전체", "♾️"), ("커스텀", "⭐"), ("월드", "🌐"),
                        ("길드", "🛡️"), ("파티", "⚔️"), ("로컬", "📍"),
                    ];
                    if signals.show_system_tab.get() { tabs.push(("시스템", "⚙️")); }

                    tabs.into_iter().map(|(full, icon)| {
                        let t_full = full.to_string();
                        let t_click = t_full.clone();
                        let is_active = move || signals.active_tab.get() == t_full;

                        view! {
                            <button
                                class="join-item btn btn-xs h-7 px-3 border-none transition-all font-black"
                                class:btn-success=is_active.clone()
                                class:bg-transparent=move || !is_active()
                                on:click=move |_| {
                                    signals.set_active_tab.set(t_click.clone());
                                    actions.save_config.dispatch(());
                                }
                            >
                                <span class="mr-1">{icon}</span>
                                <span class="hidden sm:inline">{full}</span>
                            </button>
                        }
                    }).collect_view()
                }}
            </div>

            // --- RIGHT: Control Icons & Dictionary ---
            <div class="flex items-center gap-1">

                // 📘 Dictionary Sync Button (Restored Features)
                <div class="tooltip tooltip-bottom" data-tip="Update Dictionary">
                    <button
                        class="btn btn-ghost btn-xs gap-2 relative"
                        disabled=move || is_syncing.get()
                        on:click=move |_| {
                            sync_dict_action.dispatch(());
                            signals.set_dict_update_available.set(false);
                        }
                    >
                        {move || if is_syncing.get() {
                            view! {
                                <>
                                    <span class="loading loading-spinner loading-xs text-success"></span>
                                    <span class="text-[10px] font-bold">"동기화 중..."</span>
                                </>
                            }.into_any()
                        } else {
                            view! { <span class="text-lg">"📘"</span> }.into_any()
                        }}

                        // The Update Badge (Blue Protocol style)
                        <Show when=move || signals.dict_update_available.get()>
                            <span class="absolute top-0 right-0 w-2 h-2 bg-info rounded-full animate-ping"></span>
                            <span class="absolute top-0 right-0 w-2 h-2 bg-info rounded-full"></span>
                        </Show>
                    </button>
                </div>

                <div class="divider divider-horizontal mx-0 opacity-10"></div>

                // Compact Mode Toggle
                <div class="tooltip tooltip-bottom" data-tip="Compact Mode">
                    <button class="btn btn-ghost btn-xs text-lg"
                        on:click=move |_| {
                            signals.set_compact_mode.update(|b| *b = !*b);
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