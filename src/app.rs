use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use leptos::prelude::*;
use serde_wasm_bindgen::to_value;
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
struct ModelStatus {
    exists: bool,
    path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProgressPayload {
    current: u64,
    total: u64,
    percent: u8,
}

// We only send the variant key now, not the full URL
#[derive(Serialize, Deserialize)]
struct VariantArgs {
    variant: String,
}

#[derive(Serialize, Deserialize)]
struct StartArgs {
    variant: String,
    use_gpu: bool,
}

#[derive(Serialize, Deserialize)]
struct GreetArgs<'a> {
    name: &'a str,
}

#[derive(Serialize, Deserialize)]
struct CheckModelArgs<'a> {
    filename: &'a str,
}

#[derive(Serialize, Deserialize)]
struct DownloadArgs<'a> {
    filename: &'a str,
    url: &'a str,
}

// --- CONSTANTS ---
const VARIANT_STD: &str = "std";
const VARIANT_LITE: &str = "lite";
const FILENAME_0_5B: &str = "qwen2.5-0.5b-instruct.gguf";
const URL: &str = "https://qwen2.5-0.5b-instruct.gguf";

#[component]
pub fn App() -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let (greet_msg, set_greet_msg) = signal(String::new());
    // --- SIGNALS ---
    let (status_text, set_status_text) = signal("Checking System...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);
    // Default to Standard model
    let (active_variant, set_active_variant) = signal(VARIANT_STD.to_string());

    // Run check on mount
    Effect::new(move |_| {
        spawn_local(async {
            let args = serde_wasm_bindgen::to_value(&CheckModelArgs { filename: FILENAME_0_5B }).unwrap();
            invoke("check_model_status", args).await;
        });
    });

    // 2. Download Model
    let start_download = move |variant: String| {
        println!("start_download called with {}", variant);
        set_downloading.set(true);
        set_status_text.set("Initializing Download...".to_string());
        set_progress.set(0);

        spawn_local(async move {
            // Setup Listener
            let closure = Closure::wrap(Box::new(move |payload: JsValue| {
                if let Ok(p) = serde_wasm_bindgen::from_value::<ProgressPayload>(payload) {
                    set_progress.set(p.percent);
                    set_status_text.set(format!("Downloading... {}%", p.percent));
                }
            }) as Box<dyn FnMut(JsValue)>);

            // Await listener registration to avoid race condition
            let _ = listen("download-progress", &closure).await;
            closure.forget();

            // Start Download
            let args = to_value(&serde_json::json!({ "variant": variant })).unwrap();

            match invoke("download_model", args).await {
                Ok(_) => {
                    set_downloading.set(false);
                    set_model_ready.set(true);
                    set_status_text.set("Download Complete! Ready.".to_string());
                    set_progress.set(100);
                }
                Err(e) => {
                    set_downloading.set(false);
                    set_status_text.set(format!("Download Failed: {:?}", e));
                }
            }
        });
    };

    // --- VIEW ---
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
                // Setup Options
                <Show when=move || !model_ready.get() && !downloading.get()>
                    <div class="download-options">
                        <p class="hint">"Select a model to download:"</p>
                        <button on:click=move |_| start_download(VARIANT_STD.to_string())>
                            "Standard (1.7B) - High Quality"
                        </button>
                        <button class="secondary" on:click=move |_| start_download(VARIANT_LITE.to_string())>
                            "Lite (0.5B) - Low Specs"
                        </button>
                    </div>
                </Show>
            </div>

            <style>
                "
                body { margin: 0; background: #222; overflow: hidden; }
                .container { font-family: 'Segoe UI', sans-serif; text-align: center; padding: 2rem; color: #fff; height: 100vh; display: flex; flex-direction: column; justify-content: center; }
                h1 { margin-bottom: 2rem; color: #00ff88; text-transform: uppercase; letter-spacing: 2px; }
                .status-card { background: #333; padding: 1.5rem; border-radius: 12px; margin: 0 auto 2rem; width: 80%; max-width: 400px; box-shadow: 0 8px 16px rgba(0,0,0,0.3); }
                .progress-bar { width: 100%; height: 8px; background: #444; border-radius: 4px; margin-top: 15px; overflow: hidden; }
                .fill { height: 100%; background: #00ff88; transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1); }
                .hint { color: #aaa; margin-bottom: 10px; font-size: 0.9rem; }
                .controls button { display: block; width: 80%; max-width: 300px; margin: 12px auto; padding: 14px; cursor: pointer; border-radius: 6px; font-size: 1rem; font-weight: 600; border: none; transition: transform 0.1s, opacity 0.2s; }
                .controls button:hover { opacity: 0.9; }
                .controls button:active { transform: scale(0.98); }
                .primary-btn { background: #00ff88; color: #1a1a1a; box-shadow: 0 0 15px rgba(0, 255, 136, 0.3); }
                .secondary { background: #444; color: #ddd; border: 1px solid #555; }
                .spacer { height: 8px; }
                "
            </style>
        </main>
    }
}