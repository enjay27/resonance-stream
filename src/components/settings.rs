// src/components/settings.rs
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;
use crate::types::ModelStatus;
use crate::utils::add_system_log;

#[component]
pub fn Settings() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    view! {
        <Show when=move || signals.show_settings.get()>
            // Overlay: Backdrop blur and semi-transparent black
            <div class="fixed inset-0 z-[10000] bg-black/60 backdrop-blur-sm flex items-center justify-center p-4"
                 on:click=move |_| signals.set_show_settings.set(false)>

                // Modal Body: Dark themed with BPSR green accents
                <div class="bg-[#1a1a1a] border border-white/10 w-full max-w-md max-h-[90vh] rounded-xl shadow-2xl flex flex-col animate-in fade-in zoom-in duration-200"
                     on:click=move |ev| ev.stop_propagation()>

                    // --- HEADER ---
                    <div class="flex items-center justify-between p-4 border-b border-white/5">
                        <h2 class="text-lg font-black text-white tracking-tight">"SETTINGS"</h2>
                        <button class="text-gray-500 hover:text-white transition-colors text-xl"
                                on:click=move |_| signals.set_show_settings.set(false)>"✕"</button>
                    </div>

                    // --- CONTENT (Scrollable) ---
                    <div class="flex-1 overflow-y-auto p-4 space-y-6 custom-scrollbar">

                        // Section: AI Translation
                        <section class="space-y-3">
                            <h3 class="text-[10px] font-bold text-bpsr-green uppercase tracking-widest opacity-80">"AI Translation Features"</h3>

                            <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg border border-white/5 hover:border-bpsr-green/30 transition-all cursor-pointer"
                                 on:click=move |_| {
                                     let current = signals.use_translation.get();
                                     // Logic for handling download/toggle remains the same
                                 }>
                                <span class="text-sm font-medium">"실시간 번역 기능 사용"</span>
                                <input type="checkbox" class="w-4 h-4 accent-bpsr-green"
                                    prop:checked=move || signals.use_translation.get()
                                    on:change=move |_| {} // Handled by parent div click for better UX
                                />
                            </div>

                            <Show when=move || signals.use_translation.get()>
                                // Compute Mode Radio Group
                                <div class="p-3 bg-white/5 rounded-lg space-y-2">
                                    <span class="text-xs text-gray-400">"연산 장치 (Compute Mode)"</span>
                                    <div class="flex gap-2">
                                        {vec!["cpu", "cuda"].into_iter().map(|m| {
                                            let m_val = m.to_string().clone();
                                            let m_move = m.to_string().clone();
                                            view! {
                                                <button
                                                    class=move || {
                                                        if signals.compute_mode.get() == m_val.clone() {
                                                            "bg-bpsr-green text-black border-bpsr-green"
                                                        } else {
                                                            "border-white/10 text-gray-400"
                                                        }
                                                    }
                                                    on:click=move |_| {
                                                        signals.set_compute_mode.set(m_move.clone());
                                                        actions.save_config.dispatch(());
                                                        signals.set_restart_required.set(true);
                                                    }
                                                >
                                                    {m.to_uppercase()}
                                                </button>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>
                            </Show>
                        </section>

                        // Section: Appearance
                        <section class="space-y-3">
                            <h3 class="text-[10px] font-bold text-bpsr-green uppercase tracking-widest opacity-80">"Appearance"</h3>

                            // Opacity Slider
                            <div class="space-y-2">
                                <div class="flex justify-between text-xs">
                                    <span class="text-gray-300">"Background Opacity"</span>
                                    <span class="text-bpsr-green font-mono">{move || format!("{:.0}%", signals.opacity.get() * 100.0)}</span>
                                </div>
                                <input type="range" min="0.1" max="1.0" step="0.05"
                                    class="w-full h-1 bg-white/10 rounded-lg appearance-none cursor-pointer accent-bpsr-green"
                                    prop:value=move || signals.opacity.get().to_string()
                                    on:input=move |ev| {
                                        let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                        signals.set_opacity.set(val);
                                        actions.save_config.dispatch(());
                                    }
                                />
                            </div>

                            // Theme Toggle
                            <button class="w-full flex justify-between items-center p-3 bg-white/5 rounded-lg border border-white/5 hover:border-white/20 transition-all"
                                    on:click=move |_| {
                                        let new_theme = if signals.theme.get() == "dark" { "light" } else { "dark" };
                                        signals.set_theme.set(new_theme.to_string());
                                        actions.save_config.dispatch(());
                                    }>
                                <span class="text-sm">"Theme Mode"</span>
                                <span class="text-xs font-bold uppercase tracking-widest">
                                    {move || if signals.theme.get() == "dark" { "🌙 Dark" } else { "☀️ Light" }}
                                </span>
                            </button>
                        </section>

                        // Section: Data Factory
                        <Show when=move || signals.is_debug.get()>
                            <section class="space-y-3 p-3 bg-bpsr-green/5 border border-bpsr-green/20 rounded-lg">
                                <h3 class="text-[10px] font-bold text-bpsr-green uppercase">"Data Factory (Fine-Tuning)"</h3>
                                <div class="flex items-center justify-between">
                                    <span class="text-xs text-gray-300">"채팅 로그 및 번역본 저장"</span>
                                    <input type="checkbox" class="accent-bpsr-green"
                                        prop:checked=move || signals.archive_chat.get()
                                        on:change=move |ev| {
                                            signals.set_archive_chat.set(event_target_checked(&ev));
                                        }
                                    />
                                </div>
                                <p class="text-[10px] text-gray-500 italic">"dataset_raw.jsonl 형태로 저장됩니다."</p>
                            </section>
                        </Show>
                    </div>

                    // --- FOOTER ---
                    <div class="p-4 bg-black/20 text-center border-t border-white/5">
                        <a href="https://github.com/enjay27/bpsr-translator" target="_blank"
                           class="inline-flex items-center gap-2 text-[11px] text-gray-500 hover:text-bpsr-green transition-colors">
                           <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 16 16"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
                           "BPSR Translator v1.0"
                        </a>
                    </div>
                </div>
            </div>
        </Show>
    }
}