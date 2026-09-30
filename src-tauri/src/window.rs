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

/// Grows the window to at least `min_width` x `min_height` logical pixels,
/// staying on its monitor's work area (the settings view wants more room than
/// the overlay usually has). Returns the rect it replaced, for
/// `restore_window`; `None` when nothing changed.
#[tauri::command]
pub fn grow_window(
    window: tauri::Window,
    min_width: f64,
    min_height: f64,
) -> Option<resonance_types::WindowRect> {
    if window.is_maximized().unwrap_or(false) {
        return None;
    }
    let monitor = window.current_monitor().ok().flatten()?;
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();

    let current = resonance_types::WindowRect {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    };
    let work_area = resonance_types::WindowRect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    };
    let grown = resonance_core::window::grow_to_fit(
        current,
        (min_width * scale).round() as u32,
        (min_height * scale).round() as u32,
        work_area,
    )?;
    apply_rect(&window, grown);
    Some(current)
}

/// Puts the window back where `grow_window` found it.
#[tauri::command]
pub fn restore_window(window: tauri::Window, rect: resonance_types::WindowRect) {
    apply_rect(&window, rect);
}

fn apply_rect(window: &tauri::Window, rect: resonance_types::WindowRect) {
    let _ = window.set_size(tauri::PhysicalSize::new(rect.width, rect.height));
    let _ = window.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
}
