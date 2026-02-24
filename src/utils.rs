use wasm_bindgen::prelude::*;

pub fn format_time(ts: u64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64(ts as f64 * 1000.0));
    format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
}

pub fn is_japanese(text: &str) -> bool {
    let re = js_sys::RegExp::new("[\\u3040-\\u309F\\u30A0-\\u30FF\\u4E00-\\u9FAF]", "");
    re.test(text)
}

pub fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(text);
    }
}