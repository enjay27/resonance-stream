// src-ui/src/app.rs
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    async fn listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;
}

// --- DATA STRUCTURES ---

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ModelStatus { exists: bool, path: String }

// Wrapper to handle Tauri Event Object structure
#[derive(Serialize, Deserialize, Clone, Debug)]
struct TauriEvent {
    payload: ProgressPayload,
}

// Use f64 to match JS numbers safely
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProgressPayload {
    pub current: f64,
    pub total: f64,
    pub percent: u8,
}

#[component]
pub fn App() -> impl IntoView {
    // --- STATE SIGNALS ---
    let (status_text, set_status_text) = signal("System Check...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);

    // Test Zone Signals
    let (test_input, set_test_input) = signal("".to_string());
    let (translation_log, set_translation_log) = signal("".to_string());

    // --- LOGIC: CHECK MODEL ---
    let check_model = move || {
        spawn_local(async move {
            match invoke("check_model_status", JsValue::NULL).await {
                Ok(result) => {
                    if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(result) {
                        set_model_ready.set(status.exists);
                        if status.exists {
                            set_status_text.set("Qwen 3 (0.6B) Ready".to_string());
                        } else {
                            set_status_text.set("Model Missing".to_string());
                        }
                    }
                }
                Err(e) => set_status_text.set(format!("Error: {:?}", e)),
            }
        });
    };

    // --- LOGIC: DOWNLOAD MODEL ---
    let start_download = move |_| {
        set_downloading.set(true);
        set_status_text.set("Initializing...".to_string());

        spawn_local(async move {
            // Listen for progress events
            let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                match serde_wasm_bindgen::from_value::<TauriEvent>(event_obj) {
                    Ok(wrapper) => {
                        let p = wrapper.payload;
                        set_progress.set(p.percent);
                        set_status_text.set(format!("Downloading... {}%", p.percent));
                    },
                    Err(e) => {
                        web_sys::console::error_1(&format!("Parse Error: {:?}", e).into());
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            let _ = listen("download-progress", &closure).await;
            closure.forget();

            match invoke("download_model", JsValue::NULL).await {
                Ok(_) => {
                    set_downloading.set(false);
                    set_model_ready.set(true);
                    set_status_text.set("Download Complete".to_string());
                }
                Err(e) => {
                    set_downloading.set(false);
                    set_status_text.set(format!("Failed: {:?}", e));
                }
            }
        });
    };

    // --- LOGIC: LAUNCH AI ---
    let launch_sidecar = move |_| {
        set_status_text.set("Booting AI Engine...".to_string());
        spawn_local(async move {
            // Note: "useGpu" must be camelCase for Tauri to map to snake_case in Rust
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "useGpu": true })).unwrap();

            match invoke("start_translator_sidecar", args).await {
                Ok(_) => set_status_text.set("AI Running. Ready to translate.".to_string()),
                Err(e) => set_status_text.set(format!("Launch Failed: {:?}", e)),
            }
        });
    };

    // --- LOGIC: LISTEN FOR TRANSLATIONS ---
    let setup_listener = move || {
        spawn_local(async move {
            let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                // If it's a string, display it. If it's an object, dump it.
                if let Some(str_val) = event_obj.as_string() {
                    set_translation_log.set(str_val);
                } else {
                    set_translation_log.set(format!("{:?}", event_obj));
                }
            }) as Box<dyn FnMut(JsValue)>);

            let _ = listen("translator-event", &closure).await;
            closure.forget();
        });
    };

    // --- LOGIC: MANUAL TEST SEND ---
    let send_test = move |_| {
        spawn_local(async move {
            // CHANGE THIS LINE
            // Old: let text_val = test_input.get();
            // New: Use .get_untracked() to safely read the value inside async
            let text_val = test_input.get_untracked();

            if text_val.trim().is_empty() { return; }

            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "text": text_val })).unwrap();

            // ... keep the rest the same ...
            match invoke("manual_translate", args).await {
                Ok(_) => {},
                Err(e) => set_translation_log.set(format!("Send Error: {:?}", e)),
            }
        });
    };

    // Run startup checks
    Effect::new(move |_| {
        check_model();
        setup_listener();
    });

    view! {
        <main class="container">
            <h1>"BPSR Translator"</h1>
            <p class="subtitle">"Powered by Qwen 3 (0.6B) Nano"</p>

            <div class="status-card">
                <p><strong>"Status: "</strong> {move || status_text.get()}</p>

                <Show when=move || downloading.get() fallback=|| view! { <div class="spacer"></div> }>
                    <div class="progress-bar">
                        <div class="fill" style:width=move || format!("{}%", progress.get())></div>
                    </div>
                </Show>
            </div>

            <div class="controls">
                <Show when=move || !model_ready.get() && !downloading.get()>
                    <button class="primary-btn" on:click=start_download>
                        "Download Model (450MB)"
                    </button>
                </Show>

                <Show when=move || model_ready.get()>
                    <button class="primary-btn" on:click=launch_sidecar>
                        "Start AI Translator"
                    </button>
                </Show>
            </div>

            <hr style="margin: 30px 0; border-color: #333;"/>

            // --- MANUAL TEST ZONE ---
            <div class="test-zone">
                <h3>"Manual Translator Test"</h3>
                <div class="input-group">
                    <input
                        type="text"
                        placeholder="Type Japanese (e.g. こんにちは)..."
                        on:input=move |ev| set_test_input.set(event_target_value(&ev))
                        prop:value=move || test_input.get()
                    />
                    <button class="test-btn" on:click=send_test>"Translate"</button>
                </div>

                <div class="log-box">
                    <pre>{move || translation_log.get()}</pre>
                </div>
            </div>

            <style>
                "
                body { margin: 0; background: #1a1a1a; color: #fff; font-family: 'Segoe UI', sans-serif; }
                .container { text-align: center; padding: 2rem; max-width: 600px; margin: 0 auto; }
                h1 { margin-bottom: 0.5rem; color: #00ff88; text-transform: uppercase; letter-spacing: 2px; }
                .subtitle { color: #888; margin-bottom: 2rem; }

                .status-card { background: #2a2a2a; padding: 1.5rem; border-radius: 8px; margin-bottom: 20px; box-shadow: 0 4px 6px rgba(0,0,0,0.3); }
                .progress-bar { width: 100%; height: 10px; background: #444; border-radius: 5px; overflow: hidden; margin-top: 10px; }
                .fill { height: 100%; background: #00ff88; transition: width 0.2s; }

                .controls { display: flex; justify-content: center; gap: 10px; }
                .primary-btn { background: #00ff88; border: none; padding: 15px 30px; font-size: 1.1rem; font-weight: bold; cursor: pointer; border-radius: 5px; color: #000; transition: transform 0.1s; }
                .primary-btn:active { transform: scale(0.98); }

                .test-zone { background: #222; padding: 20px; border-radius: 8px; border: 1px solid #333; }
                .input-group { display: flex; gap: 10px; justify-content: center; margin-bottom: 15px; }
                input { padding: 12px; border-radius: 4px; border: 1px solid #444; width: 70%; background: #333; color: #fff; font-size: 1rem; }
                .test-btn { background: #00aaff; border: none; padding: 10px 20px; color: white; border-radius: 4px; cursor: pointer; font-weight: bold; font-size: 1rem; }
                .test-btn:hover { background: #0088cc; }

                .log-box { background: #111; padding: 15px; border-radius: 5px; text-align: left; font-family: 'Consolas', monospace; color: #0f0; min-height: 80px; white-space: pre-wrap; word-break: break-all; border: 1px solid #333; }
                .spacer { height: 10px; }
                "
            </style>
        </main>
    }
}