mod app;
pub mod types;
pub mod components;
pub mod hooks;
pub mod utils;
pub mod store;

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
