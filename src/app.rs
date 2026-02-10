use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

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

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    async fn listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;
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
    let (status_text, set_status_text) = signal("Checking System...".to_string());
    let (model_ready, set_model_ready) = signal(false);
    let (downloading, set_downloading) = signal(false);
    let (progress, set_progress) = signal(0u8);
    // Default to Standard model
    let (active_variant, set_active_variant) = signal(VARIANT_STD.to_string());

    let update_name = move |ev| {
        let v = event_target_value(&ev);
        set_name.set(v);
    };

    let greet = move |ev: SubmitEvent| {
        ev.prevent_default();
        spawn_local(async move {
            let name = name.get_untracked();
            if name.is_empty() {
                return;
            }

            let args = serde_wasm_bindgen::to_value(&GreetArgs { name: &name }).unwrap();
            // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
            let new_msg = invoke("greet", args).await.as_string().unwrap();
            set_greet_msg.set(new_msg);
        });
    };

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

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&DownloadArgs { filename: FILENAME_0_5B, url: &*URL.to_string() }).unwrap();
            invoke("download_model", args).await;
        })

        // spawn_local(async move {
        //     // Setup Listener
        //     let closure = Closure::wrap(Box::new(move |payload: JsValue| {
        //         if let Ok(p) = serde_wasm_bindgen::from_value::<ProgressPayload>(payload) {
        //             set_progress.set(p.percent);
        //             set_status_text.set(format!("Downloading... {}%", p.percent));
        //         }
        //     }) as Box<dyn FnMut(JsValue)>);
        //
        //     // Await listener registration to avoid race condition
        //     let _ = listen("download-progress", &closure).await;
        //     closure.forget();
        //
        //     // Start Download
        //     let args = to_value(&serde_json::json!({ "variant": variant })).unwrap();
        //
        //     match invoke("download_model", args).await {
        //         Ok(_) => {
        //             set_downloading.set(false);
        //             set_model_ready.set(true);
        //             set_status_text.set("Download Complete! Ready.".to_string());
        //             set_progress.set(100);
        //         }
        //         Err(e) => {
        //             set_downloading.set(false);
        //             set_status_text.set(format!("Download Failed: {:?}", e));
        //         }
        //     }
        // });
    };

    view! {
        <main class="container">
            <h1>"Welcome to Tauri + Leptos"</h1>

            <div class="row">
                <a href="https://tauri.app" target="_blank">
                    <img src="public/tauri.svg" class="logo tauri" alt="Tauri logo"/>
                </a>
                <a href="https://docs.rs/leptos/" target="_blank">
                    <img src="public/leptos.svg" class="logo leptos" alt="Leptos logo"/>
                </a>
            </div>
            <p>"Click on the Tauri and Leptos logos to learn more."</p>

            <form class="row" on:submit=greet>
                <input
                    id="greet-input"
                    placeholder="Enter a name..."
                    on:input=update_name
                />
                <button type="submit">"Greet"</button>
            </form>
            <p>{ move || greet_msg.get() }</p>

        <div class="controls">
               <button on:click=move |_| start_download(VARIANT_STD.to_string())>
                            "Standard (1.7B) - High Quality"
                        </button>
            </div>
        </main>
    }
}