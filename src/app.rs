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

#[derive(Serialize, Deserialize, Clone)]
struct ModelStatus { exists: bool, path: String }

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TauriEvent {
    payload: ProgressPayload, // We extract this inner part
}

// Keep this one as f64 to be safe with JS numbers
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProgressPayload {
    pub current: f64,
    pub total: f64,
    pub percent: u8,
}

#[component]
pub fn App() -> impl IntoView {
    let (status_text, set_status_text) = signal("Checking System...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);
    let (test_input, set_test_input) = signal("".to_string()); // New Signal
    let (translation_log, set_translation_log) = signal("".to_string()); // To show results

    // 1. Check Model
    let check_model = move || {
        spawn_local(async move {
            match invoke("check_model_status", JsValue::NULL).await {
                Ok(result) => {
                    if let Ok(status) = serde_wasm_bindgen::from_value::<ModelStatus>(result) {
                        set_model_ready.set(status.exists);
                        if status.exists {
                            set_status_text.set("Model Ready. (Sidecar Logic Disabled)".to_string());
                        } else {
                            set_status_text.set("Model Missing".to_string());
                        }
                    }
                }
                Err(e) => set_status_text.set(format!("Error: {:?}", e)),
            }
        });
    };

    // 2. Download
    let start_download = move |_| {
        set_downloading.set(true);
        set_status_text.set("Starting Download...".to_string());

        spawn_local(async move {
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
                    set_status_text.set("Download Complete.".to_string());
                }
                Err(e) => {
                    set_downloading.set(false);
                    set_status_text.set(format!("Failed: {:?}", e));
                }
            }
        });
    };

    // 3. Launch Sidecar
    let launch_sidecar = move |_| {
        set_status_text.set("Booting AI Engine...".to_string());
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "useGpu": true })).unwrap();

            match invoke("start_translator_sidecar", args).await {
                Ok(_) => set_status_text.set("AI Running. Check Terminal.".to_string()),
                Err(e) => set_status_text.set(format!("Launch Failed: {:?}", e)),
            }
        });
    };

    Effect::new(move |_| check_model());

    let setup_listener = move || {
        spawn_local(async move {
            let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                // Parse the "translator-event" from Rust
                // We assume Python sends JSON: { "type": "result", "translated": "..." }
                // For now, just dump the raw string to the UI to verify
                if let Some(str_val) = event_obj.as_string() {
                    // In real app, parse the JSON payload here
                    set_translation_log.set(str_val);
                } else {
                    // Fallback if event is an object
                    set_translation_log.set(format!("{:?}", event_obj));
                }
            }) as Box<dyn FnMut(JsValue)>);

            let _ = listen("translator-event", &closure).await;
            closure.forget();
        });
    };

    // Call listener on startup
    Effect::new(move |_| setup_listener());

    // NEW: Manual Send
    let send_test = move |_| {
        println!("send test");
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "text": test_input.get() })).unwrap();
            let _ = invoke("manual_translate", args).await;
        });
    };

    view! {
        <main class="container">
            <h1>"BPSR Translator"</h1>

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
                        "Download Model (400MB)"
                    </button>
                </Show>
            </div>

            <Show when=move || model_ready.get()>
                <button class="primary-btn" on:click=launch_sidecar>
                    "Start AI Translator"
                </button>
            </Show>

            // MANUAL TEST ZONE
            <div class="test-zone">
                <h3>"Manual Test"</h3>
                <input
                    type="text"
                    placeholder="Type Japanese here..."
                    on:input=move |ev| set_test_input.set(event_target_value(&ev))
                />
                <button on:click=send_test>"Send"</button>

                <div class="log-box">
                    <pre>{move || translation_log.get()}</pre>
                </div>
            </div>

            <style>
                "
                body { margin: 0; background: #222; }
                .container { font-family: 'Segoe UI', sans-serif; text-align: center; padding: 2rem; color: #fff; height: 100vh; display: flex; flex-direction: column; justify-content: center; }
                .status-card { background: #333; padding: 1.5rem; border-radius: 8px; margin: 20px auto; width: 300px; }
                .progress-bar { width: 100%; height: 10px; background: #444; border-radius: 5px; overflow: hidden; margin-top: 10px; }
                .fill { height: 100%; background: #00ff88; transition: width 0.2s; }
                .primary-btn { background: #00ff88; border: none; padding: 15px 30px; font-size: 1.1rem; font-weight: bold; cursor: pointer; border-radius: 5px; }
                .spacer { height: 10px; }
                .test-zone { margin-top: 20px; border-top: 1px solid #444; padding-top: 20px; }
                input { padding: 10px; border-radius: 4px; border: none; width: 60%; margin-right: 10px; }
                .log-box { background: #111; padding: 10px; margin-top: 10px; border-radius: 5px; text-align: left; font-family: monospace; color: #0f0; min-height: 50px; }
                "
            </style>
        </main>
    }
}