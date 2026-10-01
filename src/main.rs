mod app;
pub mod chat_view;
pub mod components;
pub mod config_signals;
pub mod favorites;
pub mod hooks;
pub mod readability;
pub mod service_state;
pub mod settings_nav;
pub mod shortcut_keys;
pub mod status_signals;
pub mod status_view;
pub mod store;
pub mod tauri_bridge;
pub mod ui_types;
pub mod utils;
pub mod view_signals;

use app::*;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| {
        view! {
            <App/>
        }
    })
}
