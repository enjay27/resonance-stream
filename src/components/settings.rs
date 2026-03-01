use leptos::leptos_dom::log;
use crate::store::{AppActions, AppSignals};
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::JsValue;
use crate::tauri_bridge::invoke;
use crate::types::{FolderStatus, NetworkInterface};

#[derive(serde::Serialize)]
struct OpenBrowserArgs {
    url: String,
}

#[component]
pub fn Settings() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    let (interfaces, set_interfaces) = signal(Vec::<NetworkInterface>::new());
    let (new_keyword, set_new_keyword) = signal(String::new());
    let (new_emphasis, set_new_emphasis) = signal(String::new());

    Effect::new(move |_| {
        if signals.show_settings.get() {
            spawn_local(async move {
                if let Ok(res) = invoke("get_network_interfaces", JsValue::NULL).await {
                    if let Ok(list) = serde_wasm_bindgen::from_value::<Vec<NetworkInterface>>(res) {
                        set_interfaces.set(list);
                    }
                }
            });
        }
    });

    let sync_dict_action = Action::new_local(|_: &()| async move {
        match invoke("sync_dictionary", JsValue::NULL).await {
            Ok(_) => "최신 상태".to_string(),
            Err(_) => "동기화 실패".to_string(),
        }
    });
    let is_syncing = sync_dict_action.pending();

    let save_chat_action = Action::new_local(move |_: &()| {
        // 1. Extract the raw chat messages from the signal map
        let logs_to_export: Vec<_> = signals.chat_log.get_untracked()
            .values()
            .map(|sig| sig.get_untracked()) // Unpack the RwSignal<ChatMessage>
            .collect();

        // 2. Send them to Tauri
        async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "logs": logs_to_export })).unwrap();

            match invoke("export_chat_log", args).await {
                Ok(_) => "저장 완료".to_string(),
                Err(_) => "저장 실패".to_string(),
            }
        }
    });
    let is_saving_chat = save_chat_action.pending();

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
                                <label class="label cursor-pointer bg-base-100 rounded-lg px-4 py-3 border border-base-content/5 hover:border-success/30 transition-all">
                                    <span class="label-text font-bold text-base-content">"실시간 번역 기능 사용"</span>
                                    <input type="checkbox" class="toggle toggle-success toggle-sm"
                                        prop:checked=move || signals.use_translation.get()
                                        on:click=move |ev| {
                                            // Prevent the browser from automatically flipping the switch
                                            ev.prevent_default();

                                            let current = signals.use_translation.get_untracked();

                                            if !current {
                                                // User is trying to turn it ON
                                                spawn_local(async move {
                                                    if let Ok(st) = invoke("check_model_status", JsValue::NULL).await {
                                                        if let Ok(status) = serde_wasm_bindgen::from_value::<FolderStatus>(st) {
                                                            if status.exists {
                                                                // Model exists -> Turn it on normally
                                                                signals.set_use_translation.set(true);
                                                                actions.save_config.dispatch(());
                                                            } else {
                                                                // Model missing -> Prompt user
                                                                if let Some(w) = web_sys::window() {
                                                                    if w.confirm_with_message("AI 모델 파일이 없습니다. 다운로드 화면으로 이동하시겠습니까?").unwrap_or(false) {
                                                                        // Redirect to Setup Wizard
                                                                        signals.set_use_translation.set(true);
                                                                        signals.set_wizard_step.set(2);
                                                                        signals.set_show_settings.set(false); // Close modal
                                                                        signals.set_init_done.set(false);     // Trigger Wizard UI
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if let Ok(st) = invoke("check_ai_server_status", JsValue::NULL).await {
                                                        if let Ok(status) = serde_wasm_bindgen::from_value::<FolderStatus>(st) {
                                                            if status.exists {
                                                                // Model exists -> Turn it on normally
                                                                signals.set_use_translation.set(true);
                                                                actions.save_config.dispatch(());
                                                            } else {
                                                                // Model missing -> Prompt user
                                                                if let Some(w) = web_sys::window() {
                                                                    if w.confirm_with_message("AI 실행 파일이 없습니다. 다운로드 화면으로 이동하시겠습니까?").unwrap_or(false) {
                                                                        // Redirect to Setup Wizard
                                                                        signals.set_use_translation.set(true);
                                                                        signals.set_wizard_step.set(2);
                                                                        signals.set_show_settings.set(false); // Close modal
                                                                        signals.set_init_done.set(false);     // Trigger Wizard UI
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                });
                                            } else {
                                                // User is trying to turn it OFF
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
                                            "⚠️ 변경 사항을 적용하려면 앱을 재시작해야 합니다."
                                        </div>
                                    </Show>
                                </div>
                            </Show>
                        </section>

                        // ==========================================
                        // SECTION: CHAT SETTINGS
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

                            <div class="form-control bg-base-200 p-3 rounded-lg border border-base-content/5">
                                <label class="label cursor-pointer p-0">
                                    <span class="label-text text-xs font-bold text-base-content/80">"컴팩트 모드에서 번역 시 원문 숨기기"</span>
                                    <input type="checkbox" class="toggle toggle-success toggle-sm"
                                        prop:checked=move || signals.hide_original_in_compact.get()
                                        on:change=move |ev| {
                                            signals.set_hide_original_in_compact.set(event_target_checked(&ev));
                                            actions.save_config.dispatch(());
                                        }
                                    />
                                </label>
                            </div>
                        </section>

                        // ==========================================
                        // SECTION: KEYWORD ALERTS
                        // ==========================================
                        <section class="space-y-4">
                            <h3 class="text-[10px] font-bold text-success uppercase tracking-widest opacity-80">"키워드 알림 (Keyword Alerts)"</h3>

                            <div class="bg-base-200 p-3 rounded-lg border border-base-content/5 space-y-3">
                                <span class="text-[11px] font-bold text-base-content/60">"등록된 단어가 채팅에 등장하면 알림을 보냅니다."</span>

                                // Input Field & Add Button
                                <div class="flex gap-2">
                                    <input type="text" class="input input-xs input-bordered flex-1 font-bold" placeholder="키워드 입력..."
                                        prop:value=move || new_keyword.get()
                                        on:input=move |ev| set_new_keyword.set(event_target_value(&ev))
                                        on:keydown=move |ev| {
                                            if ev.key() == "Enter" && !new_keyword.get_untracked().trim().is_empty() {
                                                let kw = new_keyword.get_untracked().trim().to_string();
                                                signals.set_alert_keywords.update(|list| {
                                                    if !list.contains(&kw) { list.push(kw); }
                                                });
                                                set_new_keyword.set("".to_string());
                                                actions.save_config.dispatch(());
                                            }
                                        }
                                    />
                                    <button class="btn btn-xs btn-success font-black"
                                        on:click=move |_| {
                                            let kw = new_keyword.get_untracked().trim().to_string();
                                            if !kw.is_empty() {
                                                signals.set_alert_keywords.update(|list| {
                                                    if !list.contains(&kw) { list.push(kw); }
                                                });
                                                set_new_keyword.set("".to_string());
                                                actions.save_config.dispatch(());
                                            }
                                        }>
                                        "추가"
                                    </button>
                                </div>

                                // Keyword Chips
                                <div class="flex flex-wrap gap-1 mt-2">
                                    <For each=move || signals.alert_keywords.get() key=|k| k.clone() children=move |kw| {
                                        let kw_clone = kw.clone();
                                        view! {
                                            <div class="badge badge-success badge-sm gap-1 pl-2 font-bold shadow-sm">
                                                {kw.clone()}
                                                <button class="btn btn-ghost btn-xs btn-circle h-4 w-4 min-h-0 text-[10px] hover:bg-black/20"
                                                    on:click=move |_| {
                                                        signals.set_alert_keywords.update(|list| list.retain(|x| x != &kw_clone));
                                                        actions.save_config.dispatch(());
                                                    }>
                                                    "✕"
                                                </button>
                                            </div>
                                        }
                                    } />
                                </div>

                                // Volume Slider
                                <div class="space-y-2 mt-4 pt-4 border-t border-base-content/10">
                                    <div class="flex justify-between text-[11px] font-bold">
                                        <span class="text-base-content/80">"알림음 볼륨 (Volume)"</span>
                                        <span class="text-success">{move || format!("{:.0}%", signals.alert_volume.get() * 100.0)}</span>
                                    </div>
                                    <input type="range" min="0.0" max="1.0" step="0.05"
                                        class="range range-xs range-success"
                                        prop:value=move || signals.alert_volume.get().to_string()
                                        on:input=move |ev| {
                                            // 1. Update the UI state smoothly while dragging (no sound)
                                            let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.5);
                                            signals.set_alert_volume.set(val);
                                        }
                                        on:change=move |ev| {
                                            // 2. Play the sound and save to config ONLY when the mouse click is released
                                            let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.5);
                                            actions.save_config.dispatch(());

                                            if val > 0.0 {
                                                if let Ok(audio) = web_sys::HtmlAudioElement::new_with_src("public/ping.mp3") {
                                                    audio.set_volume(val as f64);
                                                    let _ = audio.play();
                                                }
                                            }
                                        }
                                    />
                                    <div class="text-[9px] text-base-content/50">"볼륨을 0%로 설정하면 알림음이 음소거됩니다."</div>
                                </div>
                            </div>

                            // --- NEW: Emphasis Keywords ---
                            <div class="bg-base-200 p-3 rounded-lg border border-base-content/5 space-y-3 mt-4">
                                <span class="text-[11px] font-bold text-base-content/60">"강조 키워드 (Emphasis Keywords) - 채팅창에서 다른 색상으로 굵게 표시됩니다."</span>

                                <div class="flex gap-2">
                                    <input type="text" class="input input-xs input-bordered flex-1 font-bold" placeholder="강조할 단어 입력..."
                                        prop:value=move || new_emphasis.get()
                                        on:input=move |ev| set_new_emphasis.set(event_target_value(&ev))
                                        on:keydown=move |ev| {
                                            if ev.key() == "Enter" && !new_emphasis.get_untracked().trim().is_empty() {
                                                let kw = new_emphasis.get_untracked().trim().to_string();
                                                signals.set_emphasis_keywords.update(|list| {
                                                    if !list.contains(&kw) { list.push(kw); }
                                                });
                                                set_new_emphasis.set("".to_string());
                                                actions.save_config.dispatch(());
                                            }
                                        }
                                    />
                                    <button class="btn btn-xs btn-warning font-black"
                                        on:click=move |_| {
                                            let kw = new_emphasis.get_untracked().trim().to_string();
                                            if !kw.is_empty() {
                                                signals.set_emphasis_keywords.update(|list| {
                                                    if !list.contains(&kw) { list.push(kw); }
                                                });
                                                set_new_emphasis.set("".to_string());
                                                actions.save_config.dispatch(());
                                            }
                                        }>
                                        "추가"
                                    </button>
                                </div>

                                <div class="flex flex-wrap gap-1 mt-2">
                                    <For each=move || signals.emphasis_keywords.get() key=|k| k.clone() children=move |kw| {
                                        let kw_clone = kw.clone();
                                        view! {
                                            <div class="badge badge-warning badge-sm gap-1 pl-2 font-bold shadow-sm">
                                                {kw.clone()}
                                                <button class="btn btn-ghost btn-xs btn-circle h-4 w-4 min-h-0 text-[10px] hover:bg-black/20"
                                                    on:click=move |_| {
                                                        signals.set_emphasis_keywords.update(|list| list.retain(|x| x != &kw_clone));
                                                        actions.save_config.dispatch(());
                                                    }>
                                                    "✕"
                                                </button>
                                            </div>
                                        }
                                    } />
                                </div>
                            </div>

                        </section>

                        // ==========================================
                        // SECTION: APPEARANCE
                        // ==========================================
                        <section class="space-y-4">
                            <h3 class="text-[10px] font-bold text-success uppercase tracking-widest opacity-80">"Appearance"</h3>

                            // Click Through Mode
                            <div class="form-control bg-base-200 p-3 rounded-lg border border-base-content/5">
                                <label class="label cursor-pointer p-0">
                                    <div class="flex flex-col">
                                        <span class="label-text text-xs font-bold text-base-content/80">"클릭 관통 모드 (Click-Through)"</span>
                                        <span class="text-[9px] text-warning mt-1">"주의: 비활성화 하려면 시스템 트레이(우측 하단 아이콘)를 사용하세요."</span>
                                    </div>
                                    <input type="checkbox" class="toggle toggle-success toggle-sm"
                                        prop:checked=move || signals.click_through.get()
                                        on:change=move |ev| {
                                            let enabled = event_target_checked(&ev);
                                            signals.set_click_through.set(enabled);
                                            actions.save_config.dispatch(());
                                            signals.set_show_settings.set(false);

                                            spawn_local(async move {
                                                let _ = invoke("set_click_through", serde_wasm_bindgen::to_value(&serde_json::json!({ "enabled": enabled })).unwrap()).await;
                                            });
                                        }
                                    />
                                </label>
                            </div>

                            // --- DRAG TO SCROLL TOGGLE ---
                            <div class="form-control bg-base-200 p-3 rounded-lg border border-base-content/5">
                                <label class="label cursor-pointer p-0">
                                    <div class="flex flex-col">
                                        <span class="label-text text-xs font-bold text-base-content/80">"드래그 스크롤 (Drag to Scroll)"</span>
                                        <span class="text-[9px] text-base-content/60 mt-1">"마우스로 채팅창 배경을 드래그하여 위아래로 스크롤합니다."</span>
                                    </div>
                                    <input type="checkbox" class="toggle toggle-success toggle-sm"
                                        prop:checked=move || signals.drag_to_scroll.get()
                                        on:change=move |ev| {
                                            let enabled = event_target_checked(&ev);
                                            signals.set_drag_to_scroll.set(enabled);
                                            actions.save_config.dispatch(());
                                        }
                                    />
                                </label>
                            </div>

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

                                <div class="flex items-center justify-between">
                                    <div class="flex flex-col">
                                        <span class="text-xs font-bold text-base-content/80">"대화 기록 저장"</span>
                                        <span class="text-[9px] opacity-60">"현재 대화 내용을 텍스트로 내보냅니다."</span>
                                    </div>
                                    <button class="btn btn-xs btn-outline w-16"
                                        disabled=move || is_saving_chat.get()
                                        on:click=move |_| { save_chat_action.dispatch(()); }
                                    >
                                        {move || if is_saving_chat.get() {
                                            view! { <span class="loading loading-spinner loading-xs"></span> }.into_any()
                                        } else if let Some(res) = save_chat_action.value().get() {
                                            // Displays "저장 완료" (Saved) or "저장 실패" (Failed) temporarily
                                            view! { {res} }.into_any()
                                        } else {
                                            view! { "저장" }.into_any()
                                        }}
                                    </button>
                                </div>

                                <div class="divider m-0 opacity-10"></div>

                                // --- NEW: Open AppData Directory ---
                                <div class="flex items-center justify-between">
                                    <div class="flex flex-col">
                                        <span class="text-xs font-bold text-base-content/80">"앱 데이터 폴더 열기"</span>
                                        <span class="text-[9px] opacity-60">"설정 및 로그 파일이 저장된 폴더를 엽니다."</span>
                                    </div>
                                    <button class="btn btn-xs btn-outline"
                                        on:click=move |_| {
                                            spawn_local(async {
                                                let _ = invoke("open_app_data_folder", JsValue::NULL).await;
                                            });
                                        }
                                    >
                                        "폴더 열기"
                                    </button>
                                </div>

                                <div class="divider m-0 opacity-10"></div>

                                <div class="flex items-center justify-between">
                                    <div class="flex flex-col">
                                        <span class="text-xs font-bold text-warning">"디버그 모드 (Debug Mode)"</span>
                                        <span class="text-[9px] opacity-60">"시스템 탭 및 개발자 도구 활성화"</span>
                                    </div>
                                    <input type="checkbox" class="toggle toggle-warning toggle-sm"
                                        prop:checked=move || signals.debug_mode.get()
                                        on:change=move |ev| {
                                            signals.set_debug_mode.set(event_target_checked(&ev));
                                            actions.save_config.dispatch(());
                                        }
                                    />
                                </div>

                                // --- REVEALED ONLY IN DEBUG MODE ---
                                <Show when=move || signals.debug_mode.get()>
                                    <div class="p-3 bg-warning/5 border border-warning/20 rounded-lg space-y-3 mt-2 animate-in fade-in slide-in-from-top-2 duration-200">

                                        // 1. Log Level Select
                                        <div class="flex items-center justify-between">
                                            <div class="flex flex-col">
                                                <span class="text-[11px] font-bold text-base-content/80">"로그 레벨 (Log Level)"</span>
                                            </div>
                                            <select class="select select-bordered select-xs w-24 text-xs font-bold bg-base-100"
                                                prop:value=move || signals.log_level.get()
                                                on:change=move |ev| {
                                                    signals.set_log_level.set(event_target_value(&ev));
                                                    actions.save_config.dispatch(());
                                                }>
                                                <option value="trace">"TRACE"</option>
                                                <option value="debug">"DEBUG"</option>
                                                <option value="info">"INFO"</option>
                                                <option value="warn">"WARN"</option>
                                                <option value="error">"ERROR"</option>
                                            </select>
                                        </div>

                                        <div class="divider m-0 opacity-10"></div>

                                        // 2. Network Interface Manual Selection
                                        <div class="flex items-center justify-between">
                                            <div class="flex flex-col">
                                                <span class="text-[11px] font-bold text-base-content/80">"네트워크 어댑터 (Network Interface)"</span>
                                                <span class="text-[9px] text-warning/80 italic">"VPN 사용 시 패킷 캡처 실패 해결용"</span>
                                            </div>
                                            <select class="select select-bordered select-xs w-36 text-[10px] font-bold bg-base-100"
                                                prop:value=move || signals.network_interface.get()
                                                on:change=move |ev| {
                                                    signals.set_network_interface.set(event_target_value(&ev));
                                                    actions.save_config.dispatch(());
                                                    signals.set_restart_required.set(true); // Requires sniffer restart
                                                }>
                                                <option value="">"Auto-Detect (권장)"</option>
                                                <For
                                                    each=move || interfaces.get()
                                                    key=|iface| iface.ip.clone()
                                                    children=move |iface| {
                                                        view! {
                                                            <option value=iface.ip.clone()>
                                                                {format!("{} ({})", iface.name, iface.ip)}
                                                            </option>
                                                        }
                                                    }
                                                />
                                            </select>
                                        </div>

                                        // 3. Data Factory (Save Chatting Log)
                                        <div class="flex items-center justify-between">
                                            <div class="flex flex-col">
                                                <span class="text-[11px] font-black text-warning uppercase">"Data Factory"</span>
                                                <span class="text-[9px] text-base-content/60 italic">"채팅 로그 원본 저장 (dataset_raw.jsonl)"</span>
                                            </div>
                                            <input type="checkbox" class="checkbox checkbox-warning checkbox-xs"
                                                prop:checked=move || signals.archive_chat.get()
                                                on:change=move |ev| {
                                                    signals.set_archive_chat.set(event_target_checked(&ev));
                                                    actions.save_config.dispatch(());
                                                }
                                            />
                                        </div>
                                    </div>
                                </Show>
                            </div>
                        </section>
                    </div>

                    // --- FOOTER: GitHub Link ---
                    <div class="p-3 bg-base-200 text-center border-t border-base-content/5">
                        <button
                            on:click=move |_| {
                                // Call the Rust backend to open the browser
                                #[cfg(target_arch = "wasm32")]
                                spawn_local(async move {
                                    let args = serde_wasm_bindgen::to_value(&OpenBrowserArgs {
                                        url: "https://github.com/enjay27/resonance-stream".to_string(),
                                    }).unwrap();

                                    // Adjust this `invoke` call to match whatever binding
                                    // you use for your other Tauri commands!
                                    let _ = invoke("open_browser", args).await;
                                });
                            }
                            class="btn btn-ghost btn-xs gap-2 text-base-content/50 hover:text-success transition-all lowercase italic"
                        >
                            <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 16 16"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
                            "Resonance Stream v2.0"
                        </button>
                    </div>
                </div>

                // Modal Backdrop to close
                <div class="modal-backdrop bg-black/40" on:click=move |_| signals.set_show_settings.set(false)></div>
            </div>
        </Show>
    }
}