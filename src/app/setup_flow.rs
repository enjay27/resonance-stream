//! First-run wizard flow: download model + server + dictionary, then start services.

use crate::hooks::use_events::setup_event_listeners;
use crate::store::AppSignals;
use crate::tauri_bridge::{invoke, listen};
use crate::ui_types::TauriEvent;
use crate::utils::add_system_log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

/// Marks setup done, saves config, and starts the event listeners and sniffer.
pub fn finalize_setup(
    signals: AppSignals,
    save_config: Action<(), ()>,
) -> impl Fn(()) + Copy + Send + Sync + 'static {
    let AppSignals {
        set_init_done,
        set_is_sniffer_active,
        ..
    } = signals;

    move |_| {
        set_init_done.set(true);
        add_system_log("success", "Setup", "Initial configuration completed.");
        save_config.dispatch(());

        spawn_local(async move {
            add_system_log("info", "Sniffer", "Initializing packet capture...");
            setup_event_listeners(signals).await;
            set_is_sniffer_active.set(true);
            let _ = invoke("start_sniffer_command", JsValue::NULL).await;
        });
    }
}

/// Fetches the gist metadata, downloads model, server and dictionary with
/// progress, launches the translator, then calls `finalize`.
pub fn start_download(
    signals: AppSignals,
    finalize_setup: impl Fn(()) + Copy + Send + Sync + 'static,
) -> impl Fn(web_sys::MouseEvent) + Copy + Send + Sync + 'static {
    let AppSignals {
        set_status_text,
        set_model_ready,
        set_downloading,
        set_progress,
        ..
    } = signals;

    move |ev: web_sys::MouseEvent| {
        // Prevent the default button behavior if necessary
        ev.prevent_default();

        set_downloading.set(true);
        set_status_text.set("Starting Downloads...".to_string());

        spawn_local(async move {
            // FETCH THE GIST METADATA FIRST
            let update_res = invoke("check_all_updates", JsValue::NULL).await;
            let (model_url, model_version, model_hash, dict_version) = if let Ok(res) = update_res {
                if let Ok(data) =
                    serde_wasm_bindgen::from_value::<crate::ui_types::UpdateCheckResult>(res)
                {
                    (
                        data.remote_data.model.download_url,
                        data.remote_data.model.latest_version,
                        data.remote_data.model.sha256,
                        data.remote_data.dictionary.version,
                    )
                } else {
                    set_status_text.set("Error: Failed to parse update data".to_string());
                    set_downloading.set(false);
                    return;
                }
            } else {
                set_status_text.set("Error: Network check failed".to_string());
                set_downloading.set(false);
                return;
            };

            // 1. Setup the progress listener
            let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
                if let Ok(wrapper) = serde_wasm_bindgen::from_value::<TauriEvent>(event_obj) {
                    set_progress.set(wrapper.payload.total_percent);
                    // Optional: Update status text to show what is currently downloading
                    // using the `current_file` field we defined in downloader.rs
                    set_status_text.set(format!(
                        "{} ({}%)",
                        wrapper.payload.current_file, wrapper.payload.total_percent
                    ));
                }
            }) as Box<dyn FnMut(JsValue)>);
            let _ = listen("download-progress", &closure).await;

            // 2. Download the AI Model (.gguf)
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "downloadUrl": model_url,
                "version": model_version,
                "expectedHash": model_hash
            }))
            .unwrap();

            let model_result = invoke("download_model", args).await;
            if let Err(e) = model_result {
                set_downloading.set(false);
                set_status_text.set(format!("Model Error: {:?}", e));
                add_system_log(
                    "error",
                    "ModelManager",
                    &format!("Model download failed: {:?}", e),
                );
                closure.forget();
                return;
            }

            // 3. Download the AI Server (llama-server.exe via zip)
            let server_result = invoke("download_ai_server", JsValue::NULL).await;
            if let Err(e) = server_result {
                set_downloading.set(false);
                set_status_text.set(format!("Server Error: {:?}", e));
                add_system_log(
                    "error",
                    "ModelManager",
                    &format!("Server download failed: {:?}", e),
                );
                closure.forget();
                return;
            }

            // 4. Sync dictionary
            let dict_args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "version": dict_version
            }))
            .unwrap();

            let sync_dict = invoke("sync_dictionary", dict_args).await;
            if let Err(e) = sync_dict {
                set_downloading.set(false);
                set_status_text.set(format!("Dict Error: {:?}", e));
                add_system_log(
                    "error",
                    "ModelManager",
                    &format!("Sync dictionary failed: {:?}", e),
                );
                closure.forget();
                return;
            }

            // The translator starts when finalize saves the config (translation
            // goes off -> on there); starting it here too ran two servers.

            // 5. Both downloads succeeded
            set_downloading.set(false);
            set_model_ready.set(true);
            set_status_text.set("Ready".to_string());
            finalize_setup(());

            closure.forget();
        });
    }
}
