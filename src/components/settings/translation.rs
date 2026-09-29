use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use crate::ui_types::FolderStatus;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::JsValue;

/// AI translation on/off, compute mode and VRAM tier.
#[component]
pub fn TranslationSection() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    view! {
        // ==========================================
        // SECTION: AI TRANSLATION
        // ==========================================
        <section class="space-y-3">
            <h3 class="text-[10px] font-bold text-success uppercase tracking-widest opacity-80">"AI Translation Features"</h3>

            <div class="form-control">
                <label class="label cursor-pointer bg-base-100 rounded-lg px-4 py-3 border border-base-content/5 hover:border-success/30 transition-all">
                    <span class="label-text font-bold text-base-content">"실시간 번역 기능 사용"</span>
                    <input type="checkbox" class="toggle toggle-success toggle-sm"
                        prop:checked=move || signals.use_translation.get()
                        on:click=move |ev| {
                            // Prevent the browser from automatically flipping the switch
                            let is_turning_on = event_target_checked(&ev);

                            if is_turning_on {
                                // 1. Optimistically set the UI to ON so the toggle moves immediately
                                signals.set_use_translation.set(true);

                                spawn_local(async move {
                                    let mut has_error = false;

                                    // 2. Check Model Status
                                    if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                                        if let Ok(status) = serde_wasm_bindgen::from_value::<FolderStatus>(st) {
                                            if !status.exists {
                                                has_error = true;
                                                if let Some(w) = web_sys::window() {
                                                    if w.confirm_with_message("AI 모델 파일이 없습니다. 다운로드 화면으로 이동하시겠습니까?").unwrap_or(false) {
                                                        signals.set_wizard_step.set(2);
                                                        signals.set_show_settings.set(false);
                                                        signals.set_init_done.set(false);
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // 3. Check Server Status (Only if model check passed, preventing double popups!)
                                    if !has_error {
                                        if let Ok(st) = invoke("check_ai_server_status", JsValue::NULL).await {
                                            if let Ok(status) = serde_wasm_bindgen::from_value::<FolderStatus>(st) {
                                                if !status.exists {
                                                    has_error = true;
                                                    if let Some(w) = web_sys::window() {
                                                        if w.confirm_with_message("AI 실행 파일이 없습니다. 다운로드 화면으로 이동하시겠습니까?").unwrap_or(false) {
                                                            signals.set_wizard_step.set(2);
                                                            signals.set_show_settings.set(false);
                                                            signals.set_init_done.set(false);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // 4. Finalize
                                    if has_error {
                                        // Revert the toggle visually back to OFF if files are missing
                                        signals.set_use_translation.set(false);
                                    } else {
                                        // Everything exists, safely save the config
                                        actions.save_config.dispatch(());
                                    }
                                });
                            } else {
                                // User is turning it OFF (Toggle visually moves immediately)
                                signals.set_use_translation.set(false);
                                actions.save_config.dispatch(());
                            }
                        }
                    />
                </label>
            </div>

            <Show when=move || signals.use_translation.get()>
                // Compute Mode Radio Group
                <div class="p-3 bg-base-200 rounded-lg space-y-3 border border-base-content/5">
                    <span class="text-[11px] font-bold text-base-content/50 uppercase">"연산 장치 (Compute Mode)"</span>
                    <div class="join w-full">
                        {vec!["cpu", "gpu"].into_iter().map(|m| {
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

                    // Hide VRAM settings if CPU is selected
                    <Show when=move || signals.compute_mode.get() == "gpu">
                        <span class="text-[11px] font-bold text-base-content/50 uppercase block mt-3">"VRAM 사용량 (GPU Offload)"</span>
                        <div class="join w-full">
                            {vec!["low", "middle", "high", "very high"].into_iter().map(|t| {
                                let t_val = t.to_string();
                                let t_click = t.to_string();
                                let t_line = t.to_string();
                                let t_tier = t.to_string();
                                view! {
                                    <button
                                        class="join-item btn btn-xs flex-1 font-black border-base-content/10"
                                        class:btn-success=move || signals.tier.get() == t_val
                                        class:btn-outline=move || signals.tier.get() != t_line
                                        class:text-secondary=move || t_tier == "extreme"
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
                        // Updated the description to accurately reflect that it improves speed, not quality
                        <div class="text-[9px] opacity-50">"할당량이 높을수록 번역 속도가 빨라지지만 VRAM을 더 많이 소모합니다."</div>
                    </Show>

                    <Show when=move || signals.restart_required.get()>
                        <div class="text-[10px] text-warning font-bold animate-pulse mt-2 p-2 bg-warning/10 rounded">
                            "⚠️ 변경 사항 적용을 위해 AI 번역기가 재시작 됩니다. 번역을 위해 잠시 시간이 소요됩니다."
                        </div>
                    </Show>
                </div>
            </Show>
        </section>
    }
}
