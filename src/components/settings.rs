use leptos::leptos_dom::log;
use crate::store::{AppActions, AppSignals};
use leptos::prelude::*;
use wasm_bindgen::JsValue;
use crate::tauri_bridge::invoke;

#[component]
pub fn Settings() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    let sync_dict_action = Action::new_local(|_: &()| async move {
        match invoke("sync_dictionary", JsValue::NULL).await {
            Ok(_) => "최신 상태".to_string(),
            Err(_) => "동기화 실패".to_string(),
        }
    });
    let is_syncing = sync_dict_action.pending();

    view! {
        <Show when=move || signals.show_settings.get()>
            <div class="modal modal-open backdrop-blur-sm transition-all duration-300 z-[20000]">
                <div class="modal-box bg-base-300 border border-base-content/10 w-full max-w-sm p-0 overflow-hidden shadow-2xl animate-in zoom-in duration-200">

                    // --- HEADER ---
                    <div class="flex items-center justify-between p-4 border-b border-base-content/5 bg-base-200">
                        <h2 class="text-sm font-black tracking-widest text-base-content">"SETTINGS"</h2>
                        <button class="btn btn-ghost btn-xs text-xl"
                                on:click=move |_| signals.set_show_settings.set(false)>"✕"</button>
                    </div>

                    // --- CONTENT (Scrollable) ---
                    <div class="flex-1 overflow-y-auto p-4 space-y-6 custom-scrollbar max-h-[70vh]">

                        // ==========================================
                        // SECTION: AI TRANSLATION
                        // ==========================================
                        <section class="space-y-3">
                            <h3 class="text-[10px] font-bold text-success uppercase tracking-widest opacity-80">"AI Translation Features"</h3>

                            <div class="form-control">
                                <label class="label cursor-pointer bg-base-100 rounded-lg px-4 py-3 border border-base-content/5 hover:border-success/30 transition-all"
                                     on:click=move |_| {
                                         let current = signals.use_translation.get();
                                         signals.set_use_translation.set(!current);
                                         actions.save_config.dispatch(());
                                     }>
                                    <span class="label-text font-bold text-base-content">"실시간 번역 기능 사용"</span>
                                    <input type="checkbox" class="toggle toggle-success toggle-sm"
                                        prop:checked=move || signals.use_translation.get()
                                        on:change=move |_| {} // Handled by label click
                                    />
                                </label>
                            </div>

                            <Show when=move || signals.use_translation.get()>
                                // Compute Mode Radio Group
                                <div class="p-3 bg-base-200 rounded-lg space-y-3 border border-base-content/5">
                                    <span class="text-[11px] font-bold text-base-content/50 uppercase">"연산 장치 (Compute Mode)"</span>
                                    <div class="join w-full">
                                        {vec!["cpu", "cuda"].into_iter().map(|m| {
                                            let m_val = m.to_string();
                                            let m_line = m.to_string();
                                            let m_click = m.to_string();
                                            view! {
                                                <button
                                                    class="join-item btn btn-xs flex-1 font-black border-base-content/10"
                                                    class:btn-success=move || signals.compute_mode.get() == m_val
                                                    class:btn-outline=move || signals.compute_mode.get() != m_line
                                                    on:click=move |_| {
                                                        signals.set_compute_mode.set(m_click.clone());
                                                        actions.save_config.dispatch(());
                                                        signals.set_restart_required.set(true);
                                                    }
                                                >
                                                    {m.to_uppercase()}
                                                </button>
                                            }
                                        }).collect_view()}
                                    </div>
                                    <div class="text-[9px] opacity-50">"GPU 사용은 NVIDIA 그래픽카드와 CUDA Toolkit이 필요합니다."</div>

                                    // [RESTORED] Performance Tier
                                    <span class="text-[11px] font-bold text-base-content/50 uppercase block mt-3">"성능 (Performance Tier)"</span>
                                    <div class="join w-full">
                                        {vec!["low", "middle", "high", "extreme"].into_iter().map(|t| {
                                            let t_val = t.to_string();
                                            let t_click = t.to_string();
                                            let t_line = t.to_string();
                                            let t_tier = t.to_string();
                                            view! {
                                                <button
                                                    class="join-item btn btn-xs flex-1 font-black border-base-content/10"
                                                    class:btn-success=move || signals.tier.get() == t_val
                                                    class:btn-outline=move || signals.tier.get() != t_line
                                                    class:text-secondary=move || t_tier == "extreme" // Make Extreme slightly distinct
                                                    on:click=move |_| {
                                                        signals.set_tier.set(t_click.clone());
                                                        actions.save_config.dispatch(());
                                                        signals.set_restart_required.set(true);
                                                    }
                                                >
                                                    {t.to_uppercase()}
                                                </button>
                                            }
                                        }).collect_view()}
                                    </div>
                                    <div class="text-[9px] opacity-50">"성능이 높을수록 번역 품질이 좋아지지만 VRAM을 더 소모합니다."</div>

                                    <Show when=move || signals.restart_required.get()>
                                        <div class="text-[10px] text-warning font-bold animate-pulse mt-2 p-2 bg-warning/10 rounded">
                                            "⚠️ 변경 사항을 적용하려면 앱을 재시작해야 합니다."
                                        </div>
                                    </Show>
                                </div>
                            </Show>
                        </section>

                        // ==========================================
                        // SECTION: CHAT SETTINGS (RESTORED)
                        // ==========================================
                        <section class="space-y-4">
                            <h3 class="text-[10px] font-bold text-success uppercase tracking-widest opacity-80">"Chat Settings"</h3>

                            // Custom Tab Configuration
                            <div class="space-y-2 px-1">
                                <span class="text-[11px] font-bold text-base-content/60 uppercase">"커스텀 탭 필터 (Custom Tab)"</span>
                                <div class="grid grid-cols-2 gap-2">
                                    {vec!["WORLD", "GUILD", "PARTY", "LOCAL"].into_iter().map(|channel| {
                                        let ch = channel.to_string();
                                        let ch_clone = ch.clone();
                                        view! {
                                            <label class="label cursor-pointer bg-base-200 rounded p-2 border border-base-content/5 hover:bg-base-content/5 transition-colors">
                                                <span class="label-text text-xs font-bold">{channel}</span>
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

                            // Message Limit
                            <div class="flex items-center justify-between bg-base-200 p-3 rounded-lg border border-base-content/5 px-3">
                                <span class="text-xs font-bold text-base-content/80">"최대 메시지 유지 개수"</span>
                                <input type="number" class="input input-xs input-bordered w-20 text-right font-mono"
                                    prop:value=move || signals.chat_limit.get().to_string()
                                    on:input=move |ev| {
                                        let val = event_target_value(&ev).parse::<usize>().unwrap_or(1000);
                                        signals.set_chat_limit.set(val);
                                        actions.save_config.dispatch(());
                                    }
                                />
                            </div>

                            // Show System Tab
                            <div class="form-control bg-base-200 p-3 rounded-lg border border-base-content/5">
                                <label class="label cursor-pointer p-0">
                                    <span class="label-text text-xs font-bold text-base-content/80">"시스템 탭 표시 (System Tab)"</span>
                                    <input type="checkbox" class="toggle toggle-success toggle-sm"
                                        prop:checked=move || signals.show_system_tab.get()
                                        on:change=move |ev| {
                                            signals.set_show_system_tab.set(event_target_checked(&ev));
                                            actions.save_config.dispatch(());
                                        }
                                    />
                                </label>
                            </div>
                        </section>

                        // ==========================================
                        // SECTION: APPEARANCE
                        // ==========================================
                        <section class="space-y-4">
                            <h3 class="text-[10px] font-bold text-success uppercase tracking-widest opacity-80">"Appearance"</h3>

                            // Opacity Slider
                            <div class="space-y-2 px-1">
                                <div class="flex justify-between text-[11px] font-bold">
                                    <span class="text-base-content/50 uppercase">"Background Opacity"</span>
                                    <span class="text-success">{move || format!("{:.0}%", signals.opacity.get() * 100.0)}</span>
                                </div>
                                <input type="range" min="0.1" max="1.0" step="0.05"
                                    class="range range-xs range-success"
                                    prop:value=move || signals.opacity.get().to_string()
                                    on:input=move |ev| {
                                        let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                        signals.set_opacity.set(val);
                                        log!("opacity {:?}", signals.opacity.get_untracked());
                                        actions.save_config.dispatch(());
                                    }
                                />
                            </div>

                            // Theme Toggle
                            <button class="btn btn-sm btn-block justify-between bg-base-200 border-base-content/5 font-bold hover:bg-base-content/10"
                                    on:click=move |_| {
                                        let new_theme = if signals.theme.get() == "dark" { "light" } else { "dark" };
                                        signals.set_theme.set(new_theme.to_string());
                                        actions.save_config.dispatch(());
                                    }>
                                <span class="text-xs">"Theme Mode"</span>
                                <span class="text-[10px] uppercase tracking-widest opacity-70">
                                    {move || if signals.theme.get() == "dark" { "🌙 Dark" } else { "☀️ Light" }}
                                </span>
                            </button>
                        </section>

                        // ==========================================
                        // SECTION: DATA & DEVELOPER
                        // ==========================================
                        <section class="space-y-3">
                            <h3 class="text-[10px] font-bold text-warning uppercase tracking-widest opacity-80">
                                "데이터 및 개발자 (Data & Dev)"
                            </h3>

                            <div class="bg-base-200 p-3 rounded-xl border border-base-content/5 space-y-4">
                                // Sync Dictionary Option
                                <div class="flex items-center justify-between">
                                    <div class="flex flex-col">
                                        <span class="text-xs font-bold text-base-content/80">"사용자 사전 동기화"</span>
                                        <span class="text-[9px] opacity-60">"GitHub에서 최신 단어장을 불러옵니다."</span>
                                    </div>
                                    <button class="btn btn-xs btn-outline relative"
                                        class:btn-success=move || signals.dict_update_available.get()
                                        disabled=move || is_syncing.get()
                                        on:click=move |_| {
                                            sync_dict_action.dispatch(());
                                            signals.set_dict_update_available.set(false);
                                        }
                                    >
                                        <Show when=move || signals.dict_update_available.get()>
                                            <span class="absolute -top-1 -right-1 flex h-2 w-2">
                                              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-75"></span>
                                              <span class="relative inline-flex rounded-full h-2 w-2 bg-success"></span>
                                            </span>
                                        </Show>

                                        {move || if is_syncing.get() {
                                            view! { <span class="loading loading-spinner loading-xs"></span> }.into_any()
                                        } else {
                                            view! { "업데이트" }.into_any()
                                        }}
                                    </button>
                                </div>

                                <div class="divider m-0 opacity-10"></div>

                                // [RESTORED] Enable Debug Logs
                                <div class="flex items-center justify-between">
                                    <div class="flex flex-col">
                                        <span class="text-xs font-bold text-base-content/80">"디버그 로그 (Debug)"</span>
                                        <span class="text-[9px] opacity-60">"시스템 탭에 상세 로그 표시"</span>
                                    </div>
                                    <input type="checkbox" class="toggle toggle-warning toggle-sm"
                                        prop:checked=move || signals.is_debug.get()
                                        on:change=move |ev| {
                                            signals.set_is_debug.set(event_target_checked(&ev));
                                            actions.save_config.dispatch(());
                                        }
                                    />
                                </div>
                            </div>
                        </section>

                        // [RESTORED] Data Factory (Fine-Tuning)
                        <Show when=move || signals.is_debug.get()>
                            <section class="p-3 bg-warning/5 border border-warning/20 rounded-lg space-y-2">
                                <div class="flex items-center justify-between">
                                    <div class="flex flex-col">
                                        <span class="text-[11px] font-black text-warning uppercase">"Data Factory"</span>
                                        <span class="text-[9px] text-base-content/60 italic">"채팅 로그 및 번역본 저장 (dataset_raw.jsonl)"</span>
                                    </div>
                                    <input type="checkbox" class="checkbox checkbox-warning checkbox-xs"
                                        prop:checked=move || signals.archive_chat.get()
                                        on:change=move |ev| {
                                            signals.set_archive_chat.set(event_target_checked(&ev));
                                            actions.save_config.dispatch(());
                                        }
                                    />
                                </div>
                            </section>
                        </Show>
                    </div>

                    // --- FOOTER: GitHub Link ---
                    <div class="p-3 bg-base-200 text-center border-t border-base-content/5">
                        <a href="https://github.com/enjay27/bpsr-translator" target="_blank"
                           class="btn btn-ghost btn-xs gap-2 text-base-content/50 hover:text-success transition-all lowercase italic">
                           <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 16 16"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
                           "BPSR Translator v2.0"
                        </a>
                    </div>
                </div>

                // Modal Backdrop to close
                <div class="modal-backdrop bg-black/40" on:click=move |_| signals.set_show_settings.set(false)></div>
            </div>
        </Show>
    }
}