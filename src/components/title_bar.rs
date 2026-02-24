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
        // navbar provides the structural flexbox and min-height
        <div class="navbar bg-base-300/60 backdrop-blur-md min-h-8 h-8 px-2 border-b border-white/5 select-none relative" data-tauri-drag-region>

            // --- LEFT: App Title ---
            <div class="flex-1 pointer-events-none">
                <span class="text-[10px] font-black tracking-tighter text-gray-500 uppercase opacity-70">
                    "Resonance Stream"
                </span>
            </div>

            // --- CENTER: App Status (READY / INITIALIZING) ---
            <div class="absolute left-1/2 -translate-x-1/2 pointer-events-none">
                <span class="text-[10px] font-black tracking-[0.2em] text-bpsr-green uppercase animate-in fade-in duration-500">
                    {move || store.status_text.get()}
                </span>
            </div>

            // --- RIGHT: System Badges & Controls ---
            <div class="flex-none flex items-center h-full no-drag">

                // DaisyUI Badge for Sniffer Status
                <div class=move || format!(
                    "badge badge-xs gap-1.5 px-2 py-2 font-black text-[9px] mr-2 border-white/5 shadow-inner transition-all {}",
                    if store.is_sniffer_active.get() {
                        "badge-success bg-success/10 text-success border-success/20"
                    } else {
                        "badge-ghost bg-white/5 text-gray-600 border-white/10"
                    }
                )>
                    // The Pulsing Indicator Dot
                    <div class=move || format!(
                        "w-1 h-1 rounded-full {}",
                        if store.is_sniffer_active.get() { "bg-success animate-pulse shadow-[0_0_8px_#00ff88]" } else { "bg-gray-600" }
                    )></div>
                    {move || if store.is_sniffer_active.get() { "SNIFFER ON" } else { "SNIFFER OFF" }}
                </div>

                // Window Control Buttons
                <div class="flex h-8 ml-1">
                    <button class="btn btn-ghost btn-xs rounded-none h-full w-10 hover:bg-white/10"
                        on:click=move |_| { spawn_local(async { let _ = invoke("minimize_window", JsValue::NULL).await; }); }>
                        <span class="opacity-70 text-[10px]">"—"</span>
                    </button>
                    <button class="btn btn-ghost btn-xs rounded-none h-full w-10 hover:bg-error hover:text-error-content transition-colors group"
                        on:click=move |_| { spawn_local(async { let _ = invoke("close_window", JsValue::NULL).await; }); }>
                        <span class="opacity-70 group-hover:opacity-100 text-xs">"✕"</span>
                    </button>
                </div>
            </div>
        </div>
    }
}