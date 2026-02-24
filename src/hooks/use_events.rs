use crate::tauri_bridge::invoke;

pub async fn clear_backend_history() {
    let _ = invoke("clear_chat_history", wasm_bindgen::JsValue::NULL).await;
}