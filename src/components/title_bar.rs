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

                // --- ADVANCED SNIFFER BADGE ---
                <div
                    class=move || {
                        let state = store.sniffer_state.get();
                        let base = "badge badge-xs gap-1.5 px-2 py-2 font-black text-[9px] mr-2 shadow-inner transition-all";
                        match state.as_str() {
                            "Active" => format!("{} badge-success bg-success/10 text-success border-success/20", base),
                            "Error" => format!("{} badge-error bg-error/10 text-error border-error/20 cursor-pointer hover:bg-error/20", base),
                            "Off" => format!("{} badge-ghost bg-white/5 text-gray-600 border-white/10", base),
                            // Yellow for transitions: Starting, Firewall, Binding
                            _ => format!("{} badge-warning bg-warning/10 text-warning border-warning/20", base),
                        }
                    }
                    on:click=move |_| {
                        // Show error alert on click if in Error state
                        if store.sniffer_state.get() == "Error" {
                            if let Some(w) = web_sys::window() {
                                let _ = w.alert_with_message(&store.sniffer_error.get());
                            }
                        }
                    }
                >
                    // The Pulsing Indicator Dot
                    <div class=move || {
                        let state = store.sniffer_state.get();
                        let base = "w-1 h-1 rounded-full";
                        match state.as_str() {
                            "Active" => format!("{} bg-success animate-pulse shadow-[0_0_8px_#00ff88]", base),
                            "Error" => format!("{} bg-error", base),
                            "Off" => format!("{} bg-gray-600", base),
                            _ => format!("{} bg-warning animate-pulse shadow-[0_0_8px_#fbbd23]", base),
                        }
                    }></div>

                    // The Status Text
                    {move || match store.sniffer_state.get().as_str() {
                        "Active" => "SNIFFER ON".to_string(),
                        "Error" => "ERROR (CLICK)".to_string(),
                        "Off" => "SNIFFER OFF".to_string(),
                        state => state.to_uppercase(), // e.g., "FIREWALL", "BINDING"
                    }}
                </div>

                <Show when=move || store.use_translation.get()>
                    <div class=move || format!(
                        "badge badge-xs gap-1.5 px-2 py-2 font-black text-[9px] mr-2 border-white/5 shadow-inner transition-all {}",
                        if store.is_translator_active.get() {
                            "badge-success bg-success/10 text-success border-success/20"
                        } else {
                            "badge-ghost bg-white/5 text-gray-600 border-white/10"
                        }
                    )>
                        <div class=move || format!(
                            "w-1 h-1 rounded-full {}",
                            if store.is_translator_active.get() { "bg-success animate-pulse shadow-[0_0_8px_#00ff88]" } else { "bg-gray-600" }
                        )></div>
                        {move || if store.is_translator_active.get() { "번역 ON" } else { "번역 OFF" }}
                    </div>
                </Show>

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