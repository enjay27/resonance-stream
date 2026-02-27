use crate::store::AppSignals;
use crate::tauri_bridge::invoke;
use crate::types::TauriEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

#[component]
pub fn SetupWizard(finalize: Callback<()>, start_download: Callback<web_sys::MouseEvent>) -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("Signals missing");

    view! {
        <div class="flex items-center justify-center min-h-screen bg-base-100 p-6">
            <div class="card w-full max-w-md bg-base-200 shadow-2xl border border-white/5">
                <div class="card-body gap-6">
                    // --- PROGRESS STEPS ---
                    <ul class="steps steps-horizontal w-full mb-4">
                        <li class="step step-success"></li>
                        <li class=move || format!("step {}", if signals.wizard_step.get() >= 1 { "step-success" } else { "" })></li>
                        <li class=move || format!("step {}", if signals.wizard_step.get() >= 2 { "step-success" } else { "" })></li>
                    </ul>

                    {move || match signals.wizard_step.get() {
                        0 => view! {
                            <div class="space-y-4 animate-in fade-in slide-in-from-bottom-4">
                                <h1 class="text-3xl font-black tracking-tighter text-bpsr-green">"RESONANCE STREAM"</h1>
                                <p class="text-sm opacity-70">"블루 프로토콜의 게임 채팅을 실시간으로 분석하고 번역합니다."</p>
                                <div class="card-actions justify-end mt-4">
                                    <button class="btn btn-success btn-block" on:click=move |_| signals.set_wizard_step.set(1)>"시작하기"</button>
                                </div>
                            </div>
                        }.into_any(),

                        1 => view! {
                            <div class="space-y-4 animate-in fade-in">
                                <h2 class="text-lg font-bold">"빠른 설정"</h2>
                                <div class="form-control bg-base-300 p-4 rounded-xl border border-white/5">
                                    <label class="label cursor-pointer">
                                        <span class="label-text font-bold">"실시간 번역 활성화"</span>
                                        <input type="checkbox" class="toggle toggle-success"
                                            prop:checked=move || signals.use_translation.get()
                                            on:change=move |ev| signals.set_use_translation.set(event_target_checked(&ev)) />
                                    </label>
                                </div>
                                <Show when=move || signals.use_translation.get()>
                                    <div class="space-y-2">
                                        <span class="text-xs font-bold opacity-50 uppercase">"연산 장치 (Compute Mode)"</span>
                                        <div class="join w-full">
                                            <button class="join-item btn btn-sm flex-1"
                                                class:btn-success=move || signals.compute_mode.get() == "cpu"
                                                on:click=move |_| signals.set_compute_mode.set("cpu".into())>"CPU"</button>
                                            <button class="join-item btn btn-sm flex-1"
                                                class:btn-success=move || signals.compute_mode.get() == "gpu"
                                                on:click=move |_| signals.set_compute_mode.set("gpu".into())>"GPU"</button>
                                        </div>
                                    </div>
                                </Show>
                                <button class="btn btn-success btn-block"
                                    on:click=move |_| if signals.use_translation.get_untracked() { signals.set_wizard_step.set(2) } else { finalize.run(()) }>
                                    "다음"
                                </button>
                            </div>
                        }.into_any(),

                        2 => view! {
                            <div class="space-y-4 text-center">
                                <h2 class="text-lg font-bold">"AI 모델 설치"</h2>
                                <p class="text-xs opacity-60">"번역을 위해 약 1GB의 AI 모델 파일 다운로드가 필요합니다."</p>
                                <Show when=move || signals.downloading.get() fallback=move || view! {
                                    <button class="btn btn-success btn-block" on:click=move |ev| start_download.run(ev)>"다운로드 시작"</button>
                                }>
                                    <progress class="progress progress-success w-full h-4" value=move || signals.progress.get().to_string() max="100"></progress>
                                    <span class="text-xs font-mono">{move || format!("{}%", signals.progress.get())}</span>
                                </Show>
                            </div>
                        }.into_any(),
                        _ => view! { <div></div> }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}