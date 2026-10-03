//! Keeps the favorites the same in every window. The backend holds the one
//! copy: a window saves only the favorites (`save_favorites`, never the whole
//! config, so it cannot overwrite settings changed elsewhere) and hears every
//! save, its own included, as `favorites-changed`.

use crate::config_signals::ConfigSignals;
use crate::tauri_bridge::{invoke, listen};
use crate::ui_types::SystemLogLevel;
use crate::utils::add_system_log;
use leptos::task::spawn_local;
use resonance_types::FavoritesState;
use wasm_bindgen::prelude::*;

/// Saves the favorites this window holds, after a change to them.
pub fn save(config: ConfigSignals) {
    let favorites = config.favorites_state();
    spawn_local(async move {
        let args =
            serde_wasm_bindgen::to_value(&serde_json::json!({ "favorites": favorites })).unwrap();
        if let Err(e) = invoke("save_favorites", args).await {
            add_system_log(
                SystemLogLevel::Error,
                "Favorites",
                &format!("Not saved: {:?}", e),
            );
        }
    });
}

/// Takes the favorites any window saves.
pub async fn listen_for_changes(config: ConfigSignals) {
    let closure = Closure::wrap(Box::new(move |event: JsValue| {
        if let Ok(event) = serde_wasm_bindgen::from_value::<serde_json::Value>(event) {
            if let Ok(favorites) =
                serde_json::from_value::<FavoritesState>(event["payload"].clone())
            {
                config.apply_favorites(favorites);
            }
        }
    }) as Box<dyn FnMut(JsValue)>);
    listen("favorites-changed", &closure).await;
    closure.forget();
}
