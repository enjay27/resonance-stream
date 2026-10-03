//! The page of a popup window (`open_popup`): one tool, on a solid background,
//! with the app's theme. The window's own title bar carries the title and the
//! close button.

use crate::components::{CheatSheetWindow, FavoritesWindow};
use crate::favorites_sync;
use crate::store::AppSignals;
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
    // This window's own copy of the settings, from the saved config: the theme
    // is the main window's, and the favorites window checks a shortcut against
    // the tab-switch keys. Only the favorites are ever saved from here
    // (`favorites_sync::save`), never the whole config.
    let signals = AppSignals::new();
    provide_context(signals);
    spawn_local(async move {
        if let Ok(config) = invoke("load_config", JsValue::NULL).await {
            if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(config) {
                apply_theme(config.theme);
                signals.config.apply(config);
            }
        }
        favorites_sync::listen_for_changes(signals.config).await;
    });

    match kind {
        PopupKind::CheatSheet => view! { <CheatSheetWindow /> }.into_any(),
        PopupKind::Favorites => view! { <FavoritesWindow /> }.into_any(),
    }
}
