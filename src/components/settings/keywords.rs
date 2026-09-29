use crate::store::{AppActions, AppSignals};
use leptos::prelude::*;

/// Emphasis and alert keywords, alert volume. The input buffers are owned by
/// `Settings` so typed text survives closing the modal, as before the split.
#[component]
pub fn KeywordSection(
    new_keyword: ReadSignal<String>,
    set_new_keyword: WriteSignal<String>,
    new_emphasis: ReadSignal<String>,
    set_new_emphasis: WriteSignal<String>,
) -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    view! {
        // ==========================================
        // SECTION: KEYWORD
        // ==========================================
        <section class="space-y-4">

            <h3 class="text-[10px] font-bold text-success uppercase tracking-widest opacity-80">"키워드 설정 (Keyword Settings)"</h3>

            // Emphasis Keywords
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

        </section>
    }
}
