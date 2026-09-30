//! The keystrokes that paste a favorite message into the focused window.
//!
//! A favorite's global shortcut fires while the user may still hold its
//! modifiers (Ctrl+Shift+1): sending Ctrl+V then would reach the game as
//! Ctrl+Shift+V. So every modifier still down is released first, then
//! Ctrl+V is sent. The app turns these into `SendInput` calls (Windows).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Windows virtual-key codes used here.
pub const VK_CONTROL: u16 = 0x11;
pub const VK_V: u16 = 0x56;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;

/// Modifier keys that may still be held when a shortcut fires; the app asks
/// the OS which of these are down and passes them to [`paste_keystrokes`].
pub const MODIFIER_KEYS: [u16; 6] = [
    VK_LSHIFT,
    VK_RSHIFT,
    VK_LCONTROL,
    VK_RCONTROL,
    VK_LMENU,
    VK_RMENU,
];

/// A favorite's shortcut pressed again within this long is ignored, so a
/// held or double-tapped key does not spam the chat.
pub const REPEAT_WINDOW: Duration = Duration::from_millis(1500);

/// Remembers when each shortcut last pasted.
#[derive(Debug, Default)]
pub struct RepeatGuard {
    last: HashMap<String, Instant>,
}

impl RepeatGuard {
    /// True when `shortcut` may paste at `now` (and records it); false when it
    /// already pasted within [`REPEAT_WINDOW`]. Other shortcuts do not count.
    pub fn allow(&mut self, shortcut: &str, now: Instant) -> bool {
        if let Some(&last) = self.last.get(shortcut) {
            if now.saturating_duration_since(last) < REPEAT_WINDOW {
                return false;
            }
        }
        self.last.insert(shortcut.to_string(), now);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub vk: u16,
    pub up: bool,
}

impl KeyStroke {
    fn down(vk: u16) -> Self {
        Self { vk, up: false }
    }
    fn up(vk: u16) -> Self {
        Self { vk, up: true }
    }
}

/// Release every held modifier, then Ctrl+V.
pub fn paste_keystrokes(held_modifiers: &[u16]) -> Vec<KeyStroke> {
    held_modifiers
        .iter()
        .map(|&vk| KeyStroke::up(vk))
        .chain([
            KeyStroke::down(VK_CONTROL),
            KeyStroke::down(VK_V),
            KeyStroke::up(VK_V),
            KeyStroke::up(VK_CONTROL),
        ])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_shortcut_is_refused_within_the_window() {
        let mut guard = RepeatGuard::default();
        let t0 = Instant::now();
        assert!(guard.allow("Ctrl+Digit1", t0));
        assert!(!guard.allow("Ctrl+Digit1", t0 + Duration::from_millis(300)));
        // A refused press does not extend the window.
        assert!(guard.allow("Ctrl+Digit1", t0 + REPEAT_WINDOW));
    }

    #[test]
    fn other_shortcuts_are_not_held_back() {
        let mut guard = RepeatGuard::default();
        let t0 = Instant::now();
        assert!(guard.allow("Ctrl+Digit1", t0));
        assert!(guard.allow("Ctrl+Digit2", t0 + Duration::from_millis(100)));
    }

    #[test]
    fn nothing_held_is_plain_ctrl_v() {
        assert_eq!(
            paste_keystrokes(&[]),
            vec![
                KeyStroke::down(VK_CONTROL),
                KeyStroke::down(VK_V),
                KeyStroke::up(VK_V),
                KeyStroke::up(VK_CONTROL),
            ]
        );
    }

    #[test]
    fn held_modifiers_are_released_before_ctrl_v() {
        let keys = paste_keystrokes(&[VK_LSHIFT, VK_LMENU]);
        assert_eq!(keys[0], KeyStroke::up(VK_LSHIFT));
        assert_eq!(keys[1], KeyStroke::up(VK_LMENU));
        assert_eq!(keys[2], KeyStroke::down(VK_CONTROL));
        assert_eq!(keys.len(), 6);
        // Every key pressed is released again.
        for k in keys.iter().filter(|k| !k.up) {
            assert!(keys.contains(&KeyStroke::up(k.vk)));
        }
    }
}
