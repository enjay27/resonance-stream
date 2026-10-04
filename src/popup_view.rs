//! Which view a window of the app shows: the main chat window, or one of the
//! popup windows (`open_popup`).

use resonance_types::{PopupKind, POPUP_LABEL_PREFIX};

/// The popup this window is, if it is one. A Tauri window's label is the
/// truth; without one (the browser preview) `?popup=<word>` in the address
/// names it, e.g. `?popup=cheatsheet`.
pub fn popup_for(label: Option<&str>, query: &str) -> Option<PopupKind> {
    if let Some(label) = label {
        return PopupKind::from_label(label);
    }
    query
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("popup="))
        .and_then(|word| PopupKind::from_label(&format!("{POPUP_LABEL_PREFIX}{word}")))
}

/// [`popup_for`] for this window: its Tauri label and address.
pub fn current_popup() -> Option<PopupKind> {
    let window = web_sys::window()?;
    let get = |target: &wasm_bindgen::JsValue, key: &str| {
        js_sys::Reflect::get(target, &wasm_bindgen::JsValue::from_str(key)).ok()
    };
    let label = get(&window, "__TAURI_INTERNALS__")
        .and_then(|internals| get(&internals, "metadata"))
        .and_then(|metadata| get(&metadata, "currentWindow"))
        .and_then(|current| get(&current, "label"))
        .and_then(|label| label.as_string());
    let query = get(&window, "location")
        .and_then(|location| get(&location, "search"))
        .and_then(|search| search.as_string())
        .unwrap_or_default();
    popup_for(label.as_deref(), &query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_window_is_not_a_popup() {
        assert_eq!(popup_for(Some("main"), ""), None);
        assert_eq!(popup_for(None, ""), None);
    }

    #[test]
    fn a_popup_window_is_known_by_its_label() {
        assert_eq!(
            popup_for(Some("popup-cheatsheet"), ""),
            Some(PopupKind::CheatSheet)
        );
        assert_eq!(
            popup_for(Some("popup-favorites"), ""),
            Some(PopupKind::Favorites)
        );
    }

    #[test]
    fn the_label_wins_over_the_address() {
        assert_eq!(
            popup_for(Some("main"), "?popup=cheatsheet"),
            None,
            "a real window label is trusted over the query"
        );
        assert_eq!(
            popup_for(Some("popup-favorites"), "?popup=cheatsheet"),
            Some(PopupKind::Favorites)
        );
    }

    #[test]
    fn without_a_label_the_address_can_name_the_popup() {
        // The browser preview has no Tauri window label.
        assert_eq!(
            popup_for(None, "?popup=cheatsheet"),
            Some(PopupKind::CheatSheet)
        );
        assert_eq!(
            popup_for(None, "?a=1&popup=favorites&b=2"),
            Some(PopupKind::Favorites)
        );
        assert_eq!(
            popup_for(None, "popup=cheatsheet"),
            Some(PopupKind::CheatSheet)
        );
    }

    #[test]
    fn an_unknown_popup_name_is_the_main_window() {
        assert_eq!(popup_for(None, "?popup=nothing"), None);
        assert_eq!(popup_for(None, "?popup="), None);
        assert_eq!(popup_for(None, "?other=cheatsheet"), None);
    }
}
