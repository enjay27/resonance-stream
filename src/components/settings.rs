use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::IntoView;
use wasm_bindgen::JsValue;
use crate::types::ModelStatus;
use crate::utils::add_system_log;

#[component]
pub fn Settings() -> impl IntoView {
    let signals = use_context::<AppSignals>()
        .expect("AppSignals context missing");
    let actions = use_context::<AppActions>()
        .expect("AppActions context missing");

    view! {
        <Show when=move || signals.show_settings.get()>
            <div class="settings-overlay" on:click=move |_| signals.set_show_settings.set(false)>
                // Event propagation stopped manually to fix the previous error
                <div class="settings-modal" on:click=move |ev| ev.stop_propagation()>

                    // Header
                    <div class="settings-header">
                        <h2>"Settings"</h2>
                        <button class="close-btn" on:click=move |_| signals.set_show_settings.set(false)>"✕"</button>
                    </div>

                    // Content (Cleaned up)
                    <div class="settings-content">
                        <div class="setting-group">
                            <h3>"AI Translation Features"</h3>
                            <div class="toggle-row">
                                <span class="toggle-label">"실시간 번역 기능 사용"</span>
                                <input type="checkbox"
                                    prop:checked=move || signals.use_translation.get()
                                    on:change=move |ev| {
                                        let checked = event_target_checked(&ev);
                                        signals.set_use_translation.set(checked);
                                        actions.save_config.dispatch(()); // Persist choice

                                        if checked {
                                            spawn_local(async move {
                                                // 1. Verify if the model files actually exist
                                                if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                                                    if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(st) {
                                                        if status.exists {
                                                            // 2a. Model exists: Start the AI sidecar immediately
                                                            add_system_log("info", "Settings", "번역 기능이 활성화되었습니다. 엔진을 시작합니다.");
                                                            let _ = invoke("start_translator_sidecar", JsValue::NULL).await;
                                                            signals.set_status_text.set("AI Engine Starting...".to_string());
                                                        } else {
                                                            // 2b. Model missing: Forward to Download Page (Step 2)
                                                            add_system_log("warn", "Settings", "AI 모델이 없습니다. 설치 마법사로 이동합니다.");

                                                            signals.set_init_done.set(false);      // Exit main view to show Wizard fallback
                                                            signals.set_wizard_step.set(2);      // Set Wizard to the Download step
                                                            signals.set_show_settings.set(false); // Close the settings modal
                                                        }
                                                    }
                                                }
                                            });
                                        } else {
                                            let msg = "번역 기능을 비활성화했습니다.\n\n사용하지 않는 AI 모델 파일(약 1.3GB)이 디스크 공간을 차지하고 있을 수 있습니다. 파일을 삭제하시겠습니까? (폴더가 열립니다)";

                                            if window().confirm_with_message(msg).unwrap_or(false) {
                                                spawn_local(async move {
                                                    // Call backend to open the model folder
                                                    let _ = invoke("open_model_folder", JsValue::NULL).await;
                                                });
                                            }

                                            add_system_log("warn", "Settings", "번역 기능 비활성화됨. (재시작 권장)");
                                            signals.set_restart_required.set(true);
                                        }
                                    }
                                />
                            </div>

                            <Show when=move || signals.use_translation.get()>
                                <div class="setting-row">
                                    <span class="toggle-label">"연산 장치 (Compute Mode)"</span>
                                    <div class="radio-group-compact">
                                        <label class="radio-row">
                                            <input type="radio" name="mode-settings" value="cpu"
                                                checked=move || signals.compute_mode.get() == "cpu"
                                                on:change=move |_| {
                                                    signals.set_compute_mode.set("cpu".into());
                                                    actions.save_config.dispatch(());
                                                    add_system_log("warn", "Settings", "CPU 모드로 설정되었습니다. 재시작 후 적용됩니다.");
                                                    signals.set_restart_required.set(true);
                                                }
                                            />
                                            <span>"CPU"</span>
                                        </label>
                                        <label class="radio-row">
                                            <input type="radio" name="mode-settings" value="cuda"
                                                checked=move || signals.compute_mode.get() == "cuda"
                                                on:change=move |_| {
                                                    signals.set_compute_mode.set("cuda".into());
                                                    actions.save_config.dispatch(());
                                                    add_system_log("warn", "Settings", "GPU 모드로 설정되었습니다. 재시작 후 적용됩니다.");
                                                    signals.set_restart_required.set(true);
                                                }
                                            />
                                            <span>"GPU"</span>
                                        </label>
                                    </div>
                                </div>
                                <p class="hint">"GPU 사용을 위해서는 NVIDIA 그래픽카드 + CUDA Toolkit 이 필요합니다. 설치되어있지 않다면 CPU 사용을 추천합니다."</p>
                                <div class="setting-row">
                                    <span class="toggle-label">"성능"</span>
                                    <div class="radio-group-compact">
                                        {vec!["low", "middle", "high", "extreme"].into_iter().map(|t| {
                                            let t_val = t.to_string();
                                            let t_val_tier = t.to_string();
                                            view! {
                                                <label class="radio-row">
                                                    <input type="radio" name="tier"
                                                        checked=move || signals.tier.get() == t_val
                                                        on:change=move |_| {
                                                            signals.set_tier.set(t_val_tier.clone());
                                                            actions.save_config.dispatch(()); // Persist choice

                                                            let msg = format!(
                                                                "성능 티어가 '{}'(으)로 변경되었습니다.\n새로운 설정을 적용하려면 앱을 재시작해야 합니다.\n\n지금 바로 새로고침할까요?",
                                                                t_val_tier.to_uppercase()
                                                            );

                                                            if window().confirm_with_message(&msg).unwrap_or(false) {
                                                                let _ = window().location().reload(); // Immediate refresh
                                                            } else {
                                                                // Log a warning in Korean in the System tab
                                                                spawn_local(async move {
                                                                    let _ = invoke("inject_system_message", serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                        "level": "warn",
                                                                        "source": "Settings",
                                                                        "message": "새 성능 설정은 앱을 재시작한 후에 적용됩니다."
                                                                    })).unwrap()).await;
                                                                });
                                                                signals.set_restart_required.set(true); // Show a persistent warning
                                                            }
                                                        }
                                                    />
                                                    <span class:tier-extreme=move || t == "extreme">{t.to_uppercase()}</span>
                                                </label>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>
                                <p class="hint">"번역 성능이 좋아지지만 번역 시간이 오래걸리고 자원을 더 많이 소모합니다. 번역에 걸리는 시간을 보고 조정해주세요."</p>
                            </Show>
                            <h3>"Overlay Settings"</h3>
                            <div class="setting-row">
                                <span>"Background Opacity"</span>
                                <div class="slider-container">
                                    <input type="range" min="0.1" max="1.0" step="0.05"
                                        prop:value=move || signals.opacity.get().to_string()
                                        on:input=move |ev| {
                                            let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.85);
                                            signals.set_opacity.set(val);
                                            actions.save_config.dispatch(()); // Persist value
                                        }
                                    />
                                    <span class="opacity-value">{move || format!("{:.0}%", signals.opacity.get() * 100.0)}</span>
                                </div>
                            </div>
                            <h3>"Display Settings"</h3>
                            <div class="toggle-row" on:click=move |_| {
                                let new_theme = if signals.theme.get() == "dark" { "light" } else { "dark" };
                                signals.set_theme.set(new_theme.to_string());
                                actions.save_config.dispatch(()); // Persist choice
                            }>
                                <span class="toggle-label">"Theme Mode"</span>
                                <button class="theme-toggle-btn">
                                    {move || if signals.theme.get() == "dark" { "🌙 Dark" } else { "☀️ Light" }}
                                </button>
                            </div>
                            <h3>"Chat Settings"</h3>
                            <h3>"Custom Tab Config"</h3>
                            <div class="filter-grid">
                                {vec!["WORLD", "GUILD", "PARTY", "LOCAL"].into_iter().map(|channel| {
                                    let ch = channel.to_string();
                                    let ch_clone = ch.clone();
                                    view! {
                                        <label class="checkbox-row">
                                            <input type="checkbox"
                                                checked=move || signals.custom_filters.get().contains(&ch_clone)
                                                on:change=move |ev| {
                                                    let checked = event_target_checked(&ev);
                                                    signals.set_custom_filters.update(|f| {
                                                        if checked { f.push(ch.clone()); }
                                                        else { f.retain(|x| x != &ch); }
                                                    });
                                                    actions.save_config.dispatch(()); // Auto-save
                                                }
                                            />
                                            <span>{channel}</span>
                                        </label>
                                    }
                                }).collect_view()}
                            </div>
                            <div class="setting-row">
                                <span>"Message Limit"</span>
                                <input type="number"
                                    prop:value=move || signals.chat_limit.get()
                                    on:input=move |ev| {
                                        let val = event_target_value(&ev).parse::<usize>().unwrap_or(1000);
                                        signals.set_chat_limit.set(val);
                                        actions.save_config.dispatch(()); // Auto-save
                                    }
                                    class="limit-input"
                                />
                            </div>
                            <h3>"Tab Visibility"</h3>
                            <div class="toggle-row">
                                <span class="toggle-label">"Show System Tab"</span>
                                <input type="checkbox"
                                    prop:checked=move || signals.show_system_tab.get()
                                    on:change=move |ev| {
                                        let checked = event_target_checked(&ev);
                                        signals.set_show_system_tab.set(checked);
                                        actions.save_config.dispatch(());
                                    }
                                />
                            </div>
                            <h3>"Log Detail"</h3>
                            <div class="toggle-row">
                                <span class="toggle-label">"Enable Debug Logs (Technical)"</span>
                                <input type="checkbox"
                                    prop:checked=move || signals.is_debug.get()
                                    on:change=move |ev| {
                                        let checked = event_target_checked(&ev);
                                        signals.set_is_debug.set(checked);
                                        actions.save_config.dispatch(());
                                    }
                                />
                            </div>
                            <Show when=move || signals.is_debug.get()>
                                <h3>"Data Factory (Fine-Tuning)"</h3>
                                <div class="toggle-row">
                                    <span class="toggle-label">"채팅 로그 및 번역본 저장"</span>
                                    <input type="checkbox"
                                        prop:checked=move || signals.archive_chat.get() // Assuming you added this signal
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            signals.set_archive_chat.set(checked);
                                            // This will trigger the "translate_and_save" cmd in the backend
                                        }
                                    />
                                </div>
                                <p class="hint">"활성화 시 모든 번역 결과가 LoRA 학습용 dataset_raw.jsonl로 저장됩니다."</p>
                            </Show>
                            <h3>"About"</h3>
                            <p>"Blue Protocol Chat Translator v1.0"</p>
                            <a href="https://github.com/enjay27/bpsr-translator" target="_blank" class="github-link">
                                <svg viewBox="0 0 16 16" width="20" height="20" fill="currentColor">
                                    <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path>
                                </svg>
                                " GitHub Repository"
                            </a>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}