//! The page of a popup window (`open_popup`): one tool, on a solid background,
//! with the app's theme. The window's own title bar carries the title and the
//! close button.

use crate::components::CheatSheetWindow;
use crate::tauri_bridge::invoke;
use crate::ui_types::{AppConfig, Theme};
use leptos::prelude::*;
use leptos::task::spawn_local;
use resonance_types::PopupKind;
use wasm_bindgen::JsValue;

fn apply_theme(theme: Theme) {
    if let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = root.set_attribute("data-theme", theme.as_str());
    }
}

#[component]
pub fn PopupApp(kind: PopupKind) -> impl IntoView {
    apply_theme(Theme::default());
    // The same theme as the main window, from the saved config.
    spawn_local(async {
        if let Ok(config) = invoke("load_config", JsValue::NULL).await {
            if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(config) {
                apply_theme(config.theme);
            }
        }
    });

    match kind {
        PopupKind::CheatSheet => view! { <CheatSheetWindow /> }.into_any(),
        // Opened by the favorites change that follows; nothing opens it yet.
        PopupKind::Favorites => view! {
            <div class="h-screen grid place-items-center bg-base-100 text-base-content/60 text-sm">"준비 중"</div>
        }
        .into_any(),
    }
}
