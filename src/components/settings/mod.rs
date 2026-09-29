mod appearance;
mod blocked_users;
mod chat;
mod data_dev;
mod keywords;
mod translation;

use self::appearance::AppearanceSection;
use self::blocked_users::BlockedUsersSection;
use self::chat::ChatSection;
use self::data_dev::DataDevSection;
use self::keywords::KeywordSection;
use self::translation::TranslationSection;
use crate::store::AppSignals;
use crate::tauri_bridge::invoke;
use crate::ui_types::NetworkInterface;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::JsValue;

#[cfg(target_arch = "wasm32")] // only the wasm click handler builds it
#[derive(serde::Serialize)]
struct OpenBrowserArgs {
    url: String,
}

#[component]
pub fn Settings() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");

    let (interfaces, set_interfaces) = signal(Vec::<NetworkInterface>::new());
    let (new_keyword, set_new_keyword) = signal(String::new());
    let (new_emphasis, set_new_emphasis) = signal(String::new());

    Effect::new(move |_| {
        if signals.show_settings.get() {
            spawn_local(async move {
                if let Ok(res) = invoke("get_network_interfaces", JsValue::NULL).await {
                    if let Ok(list) = serde_wasm_bindgen::from_value::<Vec<NetworkInterface>>(res) {
                        set_interfaces.set(list);
                    }
                }
            });
        } else {
            signals.set_restart_required.set(false);
        }
    });

    let sync_dict_action = Action::new_local(|_: &()| async move {
        // sync_dictionary needs the gist's dictionary version: look it up first
        let version = match invoke("check_all_updates", JsValue::NULL).await {
            Ok(res) => {
                match serde_wasm_bindgen::from_value::<crate::ui_types::UpdateCheckResult>(res) {
                    Ok(data) => data.remote_data.dictionary.version,
                    Err(_) => return "동기화 실패".to_string(),
                }
            }
            Err(_) => return "동기화 실패".to_string(),
        };
        let args =
            serde_wasm_bindgen::to_value(&serde_json::json!({ "version": version })).unwrap();

        match invoke("sync_dictionary", args).await {
            Ok(_) => "최신 상태".to_string(),
            Err(_) => "동기화 실패".to_string(),
        }
    });
    let save_chat_action = Action::new_local(move |_: &()| {
        // 1. Extract the raw chat messages from the signal map
        let logs_to_export: Vec<_> = signals
            .chat
            .with_untracked(|store| store.all())
            .into_iter()
            .map(|sig| sig.get_untracked()) // Unpack the RwSignal<ChatMessage>
            .collect();

        // 2. Send them to Tauri
        async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "logs": logs_to_export }))
                .unwrap();

            match invoke("export_chat_log", args).await {
                Ok(_) => "저장 완료".to_string(),
                Err(_) => "저장 실패".to_string(),
            }
        }
    });

    view! {
        <Show when=move || signals.show_settings.get()>
            <div class="modal modal-open backdrop-blur-sm transition-all duration-300 z-[20000]">
                <div class="modal-box bg-base-300 border border-base-content/10 w-full max-w-sm p-0 overflow-hidden shadow-2xl animate-in zoom-in duration-200">

                    // --- HEADER ---
                    <div class="flex items-center justify-between p-4 border-b border-base-content/5 bg-base-200">
                        <h2 class="text-sm font-black tracking-widest text-base-content">"SETTINGS"</h2>
                        <button class="btn btn-ghost btn-xs text-xl"
                                on:click=move |_| signals.set_show_settings.set(false)>"✕"</button>
                    </div>

                    // --- CONTENT (Scrollable) ---
                    <div class="flex-1 overflow-y-auto p-4 space-y-6 custom-scrollbar max-h-[70vh]">
                        <TranslationSection />
                        <ChatSection />
                        <KeywordSection new_keyword set_new_keyword new_emphasis set_new_emphasis />
                        <AppearanceSection />
                        <BlockedUsersSection />
                        <DataDevSection interfaces sync_dict_action save_chat_action />
                    </div>

                    // --- FOOTER: GitHub Link ---
                    <div class="p-3 bg-base-200 text-center border-t border-base-content/5">
                        <button
                            on:click=move |_| {
                                // Call the Rust backend to open the browser
                                #[cfg(target_arch = "wasm32")]
                                spawn_local(async move {
                                    let args = serde_wasm_bindgen::to_value(&OpenBrowserArgs {
                                        url: "https://github.com/enjay27/resonance-stream".to_string(),
                                    }).unwrap();

                                    // Adjust this `invoke` call to match whatever binding
                                    // you use for your other Tauri commands!
                                    let _ = invoke("open_browser", args).await;
                                });
                            }
                            class="btn btn-ghost btn-xs gap-2 text-base-content/50 hover:text-success transition-all lowercase italic"
                        >
                            <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 16 16"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
                            {concat!("Resonance Stream v", env!("CARGO_PKG_VERSION"))}
                        </button>
                    </div>
                </div>

                // Modal Backdrop to close
                <div class="modal-backdrop bg-black/40" on:click=move |_| signals.set_show_settings.set(false)></div>
            </div>
        </Show>
    }
}
