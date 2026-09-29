//! Global keyboard shortcut for switching chat tabs.

use crate::{inject_system_message, SystemLogLevel};
use tauri::Emitter;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[tauri::command]
pub fn update_global_tab_shortcut(app: tauri::AppHandle, modifier: String, key: String) {
    // 1. Unregister all existing shortcuts so we don't have duplicates
    let _ = app.global_shortcut().unregister_all();

    if key.trim().is_empty() || key == "None" {
        return;
    }

    // 2. Format the string for Tauri (e.g., "Ctrl+Tab", "Alt+A")
    // Tauri expects "CommandOrControl" instead of "Ctrl"
    let tauri_mod = match modifier.as_str() {
        "Ctrl" => "CommandOrControl",
        "None" | "" => "",
        other => other,
    };

    // Tauri expects uppercase letters for standard keys
    let tauri_key = key.to_uppercase();

    let shortcut_str = if tauri_mod.is_empty() {
        tauri_key
    } else {
        format!("{}+{}", tauri_mod, tauri_key)
    };

    // 3. Register the new global shortcut
    if let Ok(shortcut) = shortcut_str.parse::<Shortcut>() {
        let _ = app
            .global_shortcut()
            .on_shortcut(shortcut, move |app_handle, shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    // When the global shortcut is pressed, tell the frontend to switch tabs!
                    let _ = app_handle.emit("global-tab-switch", ());
                }
            });
    } else {
        inject_system_message(
            &app,
            SystemLogLevel::Error,
            "Shortcut",
            format!("Failed to parse global shortcut: {}", shortcut_str),
        );
    }
}
