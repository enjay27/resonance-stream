use crate::store::AppSignals;
use crate::tauri_bridge::invoke;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::IntoView;
use wasm_bindgen::JsValue;

#[component]
pub fn TitleBar() -> impl IntoView {
    let store = use_context::<AppSignals>()
        .expect("GlobalStore context missing");

    view! {
        <div class="custom-title-bar" data-tauri-drag-region>
            <div class="drag-handle" data-tauri-drag-region></div>
            <div class="window-title" style="pointer-events: none;">
                "Resonance Stream"
            </div>

            <div class="title-bar-status">
                {move || store.status_text.get()}
            </div>

            <div class="window-controls">
                <div class="status-dot-container title-bar-version"
                     class:online=move || store.is_sniffer_active.get()>
                     <span class="pulse-dot"></span>
                     <span>{move || if store.is_sniffer_active.get() { "SNIFFER" } else { "NO SNIFFER" }}</span>
                </div>
                <Show when=move || store.use_translation.get()>
                    <div class="status-dot-container title-bar-version"
                         class:online=move || store.is_translator_active.get()>
                         <span class="pulse-dot"></span>
                         <span>{move || if store.is_translator_active.get() { "번역 ON" } else { "번역 OFF" }}</span>
                    </div>
                </Show>
                <button class="win-btn" on:click=move |_| {
                    spawn_local(async move {
                        spawn_local(async { let _ = invoke("minimize_window", JsValue::NULL).await; });
                    });
                }>"—"</button>

                <button class="win-btn close" on:click=move |_| {
                    spawn_local(async move {
                        spawn_local(async { let _ = invoke("close_window", JsValue::NULL).await; });
                    });
                }>"✕"</button>
            </div>
        </div>
    }
}
