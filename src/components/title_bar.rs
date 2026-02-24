use crate::store::AppSignals;
use crate::tauri_bridge::invoke;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::IntoView;
use wasm_bindgen::JsValue;

#[component]
pub fn TitleBar() -> impl IntoView {
    let store = use_context::<AppSignals>().expect("Store missing");

    view! {
        <div class="flex items-center justify-between h-8 bg-black/40 border-b border-white/10 select-none px-2" data-tauri-drag-region>
            <div class="absolute inset-0 z-0" data-tauri-drag-region></div>

            <div class="relative z-10 text-[10px] font-semibold text-gray-500 pointer-events-none">
                "Resonance Stream"
            </div>

            <div class="relative z-10 text-xs text-gray-300">
                {move || store.status_text.get()}
            </div>

            <div class="relative z-20 flex items-center h-full no-drag">
                // Sniffer Status Dot
                <div class="flex items-center gap-1 px-2 py-0.5 rounded bg-black/30 border border-white/5 text-[10px] font-extrabold mr-2">
                     <span class=move || format!("w-1.5 h-1.5 rounded-full {}",
                        if store.is_sniffer_active.get() { "bg-bpsr-green shadow-[0_0_6px_#00ff88] animate-pulse" } else { "bg-gray-600" }
                     )></span>
                     <span class=move || if store.is_sniffer_active.get() { "text-bpsr-green" } else { "text-gray-500" }>
                        {move || if store.is_sniffer_active.get() { "SNIFFER" } else { "OFFLINE" }}
                     </span>
                </div>

                <button class="w-11 h-full hover:bg-white/10 transition-colors" on:click=move |_| {
                    spawn_local(async { let _ = invoke("minimize_window", JsValue::NULL).await; });
                }>"—"</button>

                <button class="w-11 h-full hover:bg-red-600 hover:text-white transition-colors" on:click=move |_| {
                    spawn_local(async { let _ = invoke("close_window", JsValue::NULL).await; });
                }>"✕"</button>
            </div>
        </div>
    }
}