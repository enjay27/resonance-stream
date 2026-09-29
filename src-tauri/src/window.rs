//! Tauri commands that act on the overlay window itself.

#[tauri::command]
pub fn set_always_on_top(window: tauri::Window, on_top: bool) {
    // This simple method toggles the window state
    let _ = window.set_always_on_top(on_top);
}

#[tauri::command]
pub fn minimize_window(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
pub fn close_window(window: tauri::Window) {
    let _ = window.close();
}

#[tauri::command]
pub fn set_click_through(window: tauri::Window, enabled: bool) {
    let _ = window.set_ignore_cursor_events(enabled);
}
