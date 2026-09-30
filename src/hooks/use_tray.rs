//! Tray <-> UI wiring: tray menu clicks flip UI state, UI state relabels the tray.

use crate::config_signals::ConfigSignals;
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::{invoke, listen};
use crate::view_signals::UiSignals;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

/// Listens for the tray's click-through and always-on-top menu items.
pub fn setup_tray_listeners(signals: AppSignals, actions: AppActions) {
    let ConfigSignals {
        always_on_top: is_pinned,
        set_always_on_top: set_is_pinned,
        ..
    } = signals.config;
    let UiSignals {
        click_through,
        set_click_through,
        ..
    } = signals.ui;

    spawn_local(async move {
        // 1. Click-Through Listener (Existing)
        let tray_closure = Closure::wrap(Box::new(move |_: JsValue| {
            let current = click_through.get_untracked();
            set_click_through.set(!current);
            actions.save_config.dispatch(());

            spawn_local(async move {
                let _ = invoke(
                    "set_click_through",
                    serde_wasm_bindgen::to_value(&serde_json::json!({ "enabled": !current }))
                        .unwrap(),
                )
                .await;
            });
        }) as Box<dyn FnMut(JsValue)>);

        let _ = listen("tray-toggle-click-through", &tray_closure).await;
        tray_closure.forget();

        // 2. Always on Top Listener (NEW)
        let tray_top_closure = Closure::wrap(Box::new(move |_: JsValue| {
            let current = is_pinned.get_untracked();
            set_is_pinned.set(!current); // Flip the signal so the TitleBar icon updates!
            actions.save_config.dispatch(()); // Save to config file

            spawn_local(async move {
                // Tauri command expects { "onTop": bool }
                let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "onTop": !current }))
                    .unwrap();
                let _ = invoke("set_always_on_top", args).await;
            });
        }) as Box<dyn FnMut(JsValue)>);

        let _ = listen("tray-toggle-always-on-top", &tray_top_closure).await;
        tray_top_closure.forget();
    });
}

/// Keeps the tray menu labels in sync with click-through / always-on-top.
/// Runs on start-up and whenever either signal changes.
pub fn sync_tray_menu(signals: AppSignals) {
    let ConfigSignals {
        always_on_top: is_pinned,
        ..
    } = signals.config;
    let UiSignals { click_through, .. } = signals.ui;

    Effect::new(move |_| {
        let ct = click_through.get();
        let aot = is_pinned.get();

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "clickThrough": ct,
                "alwaysOnTop": aot
            }))
            .unwrap();

            let _ = invoke("update_tray_menu", args).await;
        });
    });
}
