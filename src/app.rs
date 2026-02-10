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

#[derive(Serialize, Deserialize, Clone)]
struct ProgressPayload { percent: u8 }

#[component]
pub fn App() -> impl IntoView {
    let (status_text, set_status_text) = signal("Checking System...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);

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
            let closure = Closure::wrap(Box::new(move |payload: JsValue| {
                if let Ok(p) = serde_wasm_bindgen::from_value::<ProgressPayload>(payload) {
                    set_progress.set(p.percent);
                    set_status_text.set(format!("Downloading... {}%", p.percent));
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

    Effect::new(move |_| check_model());

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

            <style>
                "
                body { margin: 0; background: #222; }
                .container { font-family: 'Segoe UI', sans-serif; text-align: center; padding: 2rem; color: #fff; height: 100vh; display: flex; flex-direction: column; justify-content: center; }
                .status-card { background: #333; padding: 1.5rem; border-radius: 8px; margin: 20px auto; width: 300px; }
                .progress-bar { width: 100%; height: 10px; background: #444; border-radius: 5px; overflow: hidden; margin-top: 10px; }
                .fill { height: 100%; background: #00ff88; transition: width 0.2s; }
                .primary-btn { background: #00ff88; border: none; padding: 15px 30px; font-size: 1.1rem; font-weight: bold; cursor: pointer; border-radius: 5px; }
                .spacer { height: 10px; }
                "
            </style>
        </main>
    }
}