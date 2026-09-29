//! "Update available" modals for the app and the AI model.

use crate::store::AppSignals;
use crate::tauri_bridge::{invoke, listen};
use crate::ui_types::ProgressPayload;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

/// App update: release notes, skip, download with progress, restart to apply.
#[component]
pub fn AppUpdateModal() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let AppSignals {
        set_status_text,
        show_app_update_modal,
        set_show_app_update_modal,
        pending_update_data,
        app_update_step,
        set_app_update_step,
        app_update_progress,
        set_app_update_progress,
        ..
    } = signals;

    // --- APP UPDATE LOGIC ---
    let start_app_update = move |download_url: String| {
        set_app_update_step.set(1);
        set_app_update_progress.set(0);

        spawn_local(async move {
            let progress_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(payload) =
                        serde_json::from_value::<ProgressPayload>(ev["payload"].clone())
                    {
                        if payload.current_file.contains("앱 업데이트") {
                            set_app_update_progress.set(payload.percent);
                            if payload.percent >= 100 {
                                set_app_update_step.set(2);
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("download-progress", &progress_closure).await;
            progress_closure.forget(); // Keep alive during download

            let args =
                serde_wasm_bindgen::to_value(&serde_json::json!({ "downloadUrl": download_url }))
                    .unwrap();
            let _ = invoke("download_app_update", args).await;
        });
    };

    view! {
        // ==========================================
        // APP UPDATE MODAL
        // ==========================================
        <Show when=move || show_app_update_modal.get()>
            <div class="modal modal-open backdrop-blur-sm z-[30000]">
                <div class="modal-box bg-base-300 border border-success/30">
                    <h3 class="font-black text-lg text-success mb-4">"새로운 앱 업데이트 가능!"</h3>
                    {move || pending_update_data.get().map(|data| view! {
                        <div class="space-y-4">
                            {move || match app_update_step.get() {
                                0 => view! {
                                    <div class="animate-in fade-in">
                                        <p class="text-sm font-bold">"버전: " {data.app.latest_version.clone()}</p>
                                        <div class="bg-base-200 p-3 rounded text-xs opacity-80 whitespace-pre-wrap mt-2">
                                            {data.app.release_notes.clone()}
                                        </div>
                                        <div class="modal-action">
                                            <button class="btn btn-ghost text-base-content/50"
                                                on:click={
                                                    // CLONE DATA BEFORE THE CLOSURE
                                                    let version = data.app.latest_version.clone();
                                                    move |_| {
                                                        let v = version.clone();
                                                        spawn_local(async move {
                                                            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                "target": "app",
                                                                "version": v
                                                            })).unwrap();
                                                            let _ = invoke("ignore_update", args).await;
                                                        });
                                                        set_show_app_update_modal.set(false);
                                                    }
                                                }>
                                                "이번 버전 건너뛰기"
                                            </button>
                                            <button class="btn btn-success"
                                                on:click={
                                                    // CLONE DATA BEFORE THE CLOSURE
                                                    let url = data.app.download_url.clone();
                                                    move |_| { start_app_update(url.clone()); }
                                                }>
                                                "다운로드 시작"
                                            </button>
                                        </div>
                                    </div>
                                }.into_any(),

                                1 => view! {
                                    <div class="space-y-2 py-4 animate-in fade-in text-center">
                                        <p class="text-sm font-bold opacity-80">"업데이트 파일을 다운로드 중입니다..."</p>
                                        <progress class="progress progress-success w-full h-4" value=move || app_update_progress.get().to_string() max="100"></progress>
                                        <span class="text-xs font-mono">{move || format!("{}%", app_update_progress.get())}</span>
                                    </div>
                                }.into_any(),

                                _ => view! {
                                    <div class="space-y-4 py-4 animate-in zoom-in text-center">
                                        <div class="text-4xl mb-2">"🎉"</div>
                                        <p class="text-lg font-bold text-success">"다운로드 완료!"</p>
                                        <p class="text-xs opacity-70">"새로운 버전을 적용하려면 앱을 재시작해야 합니다."</p>
                                        <button class="btn btn-success btn-block mt-4 gap-2"
                                            on:click=move |_| {
                                                set_status_text.set("재시작 중...".to_string());
                                                spawn_local(async move { let _ = invoke("restart_to_apply_update", JsValue::NULL).await; });
                                            }>
                                            "재시작 및 적용"
                                        </button>
                                    </div>
                                }.into_any(),
                            }}
                        </div>
                    })}
                </div>
            </div>
        </Show>
    }
}

/// AI model update: release notes, skip, download with progress.
#[component]
pub fn ModelUpdateModal() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let AppSignals {
        show_model_update_modal,
        set_show_model_update_modal,
        pending_update_data,
        model_update_step,
        set_model_update_step,
        model_update_progress,
        set_model_update_progress,
        ..
    } = signals;

    // --- MODEL UPDATE LOGIC ---
    let start_model_update = move |download_url: String, version: String| {
        set_model_update_step.set(1);
        set_model_update_progress.set(0);

        spawn_local(async move {
            let progress_closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(ev) = serde_wasm_bindgen::from_value::<serde_json::Value>(event_obj) {
                    if let Ok(payload) =
                        serde_json::from_value::<ProgressPayload>(ev["payload"].clone())
                    {
                        if payload.current_file.contains("AI 모델") {
                            set_model_update_progress.set(payload.percent);
                            if payload.percent >= 100 {
                                set_model_update_step.set(2);
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            listen("download-progress", &progress_closure).await;
            progress_closure.forget(); // Keep alive during download

            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "downloadUrl": download_url,
                "version": version
            }))
            .unwrap();
            let _ = invoke("download_model", args).await;
        });
    };

    view! {
        // ==========================================
        // MODEL UPDATE MODAL
        // ==========================================
        <Show when=move || show_model_update_modal.get()>
            <div class="modal modal-open backdrop-blur-sm z-[30000]">
                <div class="modal-box bg-base-300 border border-info/30">
                    <h3 class="font-black text-lg text-info mb-4">"새로운 AI 번역 모델!"</h3>
                    {move || pending_update_data.get().map(|data| view! {
                        <div class="space-y-4">
                            {move || match model_update_step.get() {
                                0 => view! {
                                    <div class="animate-in fade-in">
                                        <p class="text-sm font-bold">"버전: " {data.model.latest_version.clone()}</p>
                                        <div class="bg-base-200 p-3 rounded text-xs opacity-80 whitespace-pre-wrap mt-2">
                                            {data.model.release_notes.clone()}
                                        </div>
                                        <div class="modal-action">
                                            <button class="btn btn-ghost text-base-content/50"
                                                on:click={
                                                    // CLONE DATA BEFORE THE CLOSURE
                                                    let version = data.model.latest_version.clone();
                                                    move |_| {
                                                        let v = version.clone();
                                                        spawn_local(async move {
                                                            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                "target": "model",
                                                                "version": v
                                                            })).unwrap();
                                                            let _ = invoke("ignore_update", args).await;
                                                        });
                                                        set_show_model_update_modal.set(false);
                                                    }
                                                }>
                                                "건너뛰기"
                                            </button>
                                            <button class="btn btn-info"
                                                on:click={
                                                    // CLONE DATA BEFORE THE CLOSURE
                                                    let url = data.model.download_url.clone();
                                                    let version = data.model.latest_version.clone();
                                                    move |_| {
                                                        start_model_update(url.clone(), version.clone());
                                                    }
                                                }>
                                                "다운로드 시작 (약 2.4GB)"
                                            </button>
                                        </div>
                                    </div>
                                }.into_any(),

                                1 => view! {
                                    <div class="space-y-2 py-4 animate-in fade-in text-center">
                                        <p class="text-sm font-bold opacity-80">"AI 모델을 다운로드 중입니다..."</p>
                                        <progress class="progress progress-info w-full h-4" value=move || model_update_progress.get().to_string() max="100"></progress>
                                        <span class="text-xs font-mono">{move || format!("{}%", model_update_progress.get())}</span>
                                    </div>
                                }.into_any(),

                                _ => view! {
                                    <div class="space-y-4 py-4 animate-in zoom-in text-center">
                                        <div class="text-4xl mb-2">"✨"</div>
                                        <p class="text-lg font-bold text-info">"모델 다운로드 완료!"</p>
                                        <p class="text-xs opacity-70">"새로운 AI 모델이 디스크에 성공적으로 저장되었습니다."</p>
                                        <button class="btn btn-info btn-block mt-4 gap-2"
                                            on:click=move |_| {
                                                set_show_model_update_modal.set(false);
                                                signals.set_restart_required.set(true);
                                            }>
                                            "확인"
                                        </button>
                                    </div>
                                }.into_any(),
                            }}
                        </div>
                    })}
                </div>
            </div>
        </Show>
    }
}
