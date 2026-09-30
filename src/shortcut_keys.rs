//! Recording global shortcuts for favorite messages -- pure, host-tested.
//!
//! A shortcut is stored as a Tauri accelerator built from
//! `KeyboardEvent.code` ("Ctrl+Shift+Digit1"): the physical key, so it does
//! not change with the Korean/Japanese IME or keyboard layout, and the
//! backend's shortcut parser reads it as-is.

/// Why a key press did not become a shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejected {
    /// Only a modifier is down so far: keep recording.
    ModifierOnly,
    /// A bare key would stop typing that key in the game; only F-keys may
    /// go without a modifier.
    NeedsModifier,
    /// A key the global shortcut parser does not know.
    Unsupported,
}

const MODIFIER_CODES: [&str; 8] = [
    "ControlLeft",
    "ControlRight",
    "ShiftLeft",
    "ShiftRight",
    "AltLeft",
    "AltRight",
    "MetaLeft",
    "MetaRight",
];

fn is_function_key(code: &str) -> bool {
    code.strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n))
}

fn is_supported(code: &str) -> bool {
    let suffix_is = |prefix: &str, ok: fn(char) -> bool| {
        code.strip_prefix(prefix).is_some_and(|rest| {
            let mut chars = rest.chars();
            matches!((chars.next(), chars.next()), (Some(c), None) if ok(c))
        })
    };
    suffix_is("Key", |c| c.is_ascii_uppercase())
        || suffix_is("Digit", |c| c.is_ascii_digit())
        || suffix_is("Numpad", |c| c.is_ascii_digit())
        || is_function_key(code)
        || matches!(
            code,
            "Backquote"
                | "Minus"
                | "Equal"
                | "BracketLeft"
                | "BracketRight"
                | "Backslash"
                | "Semicolon"
                | "Quote"
                | "Comma"
                | "Period"
                | "Slash"
                | "Space"
                | "Tab"
                | "ArrowUp"
                | "ArrowDown"
                | "ArrowLeft"
                | "ArrowRight"
                | "Home"
                | "End"
                | "PageUp"
                | "PageDown"
                | "Insert"
                | "Delete"
                | "NumpadAdd"
                | "NumpadSubtract"
                | "NumpadMultiply"
                | "NumpadDivide"
                | "NumpadDecimal"
        )
}

/// The accelerator for a key press, modifiers in a fixed order.
pub fn accelerator_from_event(
    code: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> Result<String, Rejected> {
    if MODIFIER_CODES.contains(&code) {
        return Err(Rejected::ModifierOnly);
    }
    if !is_supported(code) {
        return Err(Rejected::Unsupported);
    }
    if !(ctrl || alt || shift) && !is_function_key(code) {
        return Err(Rejected::NeedsModifier);
    }
    let mut parts: Vec<&str> = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    if shift {
        parts.push("Shift");
    }
    parts.push(code);
    Ok(parts.join("+"))
}

fn key_label(code: &str) -> String {
    if let Some(c) = code.strip_prefix("Key") {
        return c.to_string();
    }
    if let Some(d) = code.strip_prefix("Digit") {
        return d.to_string();
    }
    let label = match code {
        "Backquote" => "`",
        "Minus" => "-",
        "Equal" => "=",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "NumpadAdd" => "Num +",
        "NumpadSubtract" => "Num -",
        "NumpadMultiply" => "Num *",
        "NumpadDivide" => "Num /",
        "NumpadDecimal" => "Num .",
        other => {
            if let Some(n) = other.strip_prefix("Numpad") {
                return format!("Num {n}");
            }
            other
        }
    };
    label.to_string()
}

/// "Ctrl+Shift+Digit1" -> "Ctrl + Shift + 1"; empty for no shortcut.
pub fn display(accelerator: &str) -> String {
    if accelerator.is_empty() {
        return String::new();
    }
    accelerator
        .split('+')
        .map(|part| match part {
            "Ctrl" | "Alt" | "Shift" => part.to_string(),
            code => key_label(code),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// The tab-switch shortcut (stored as modifier + `KeyboardEvent.key`) in the
/// same form, so a favorite can be checked against it. `None` when unset.
pub fn tab_switch_accelerator(modifier: &str, key: &str) -> Option<String> {
    if key.trim().is_empty() || key == "None" {
        return None;
    }
    let mut chars = key.chars();
    let code = match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => format!("Key{}", c.to_ascii_uppercase()),
        (Some(c), None) if c.is_ascii_digit() => format!("Digit{c}"),
        (Some(' '), None) => "Space".to_string(),
        (Some('`'), None) => "Backquote".to_string(),
        _ => key.to_string(),
    };
    Some(match modifier {
        "Ctrl" | "Alt" | "Shift" => format!("{modifier}+{code}"),
        _ => code,
    })
}

/// Why `candidate` cannot be used: the tab-switch shortcut, or the index of
/// another favorite (`own` is the favorite being edited, skipped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    TabSwitch,
    Favorite(usize),
}

pub fn find_conflict(
    candidate: &str,
    own: usize,
    favorites: &[String],
    tab_switch: Option<&str>,
) -> Option<Conflict> {
    if candidate.is_empty() {
        return None;
    }
    if tab_switch == Some(candidate) {
        return Some(Conflict::TabSwitch);
    }
    favorites
        .iter()
        .enumerate()
        .find(|(i, s)| *i != own && s.as_str() == candidate)
        .map(|(i, _)| Conflict::Favorite(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_come_in_a_fixed_order() {
        assert_eq!(
            accelerator_from_event("Digit1", true, true, true).unwrap(),
            "Ctrl+Alt+Shift+Digit1"
        );
        assert_eq!(
            accelerator_from_event("KeyQ", false, true, false).unwrap(),
            "Alt+KeyQ"
        );
    }

    #[test]
    fn a_bare_key_needs_a_modifier_except_f_keys() {
        assert_eq!(
            accelerator_from_event("KeyA", false, false, false),
            Err(Rejected::NeedsModifier)
        );
        assert_eq!(
            accelerator_from_event("F5", false, false, false).unwrap(),
            "F5"
        );
        assert_eq!(
            accelerator_from_event("F25", false, false, false),
            Err(Rejected::Unsupported)
        );
    }

    #[test]
    fn modifier_alone_keeps_recording_and_odd_keys_are_refused() {
        assert_eq!(
            accelerator_from_event("ShiftLeft", false, false, true),
            Err(Rejected::ModifierOnly)
        );
        assert_eq!(
            accelerator_from_event("Lang1", true, false, false),
            Err(Rejected::Unsupported)
        );
        assert_eq!(
            accelerator_from_event("KeyAB", true, false, false),
            Err(Rejected::Unsupported)
        );
    }

    #[test]
    fn display_is_readable() {
        assert_eq!(display("Ctrl+Shift+Digit1"), "Ctrl + Shift + 1");
        assert_eq!(display("Alt+Backquote"), "Alt + `");
        assert_eq!(display("Ctrl+Numpad3"), "Ctrl + Num 3");
        assert_eq!(display("F7"), "F7");
        assert_eq!(display(""), "");
    }

    #[test]
    fn tab_switch_shortcut_converts_to_the_same_form() {
        assert_eq!(
            tab_switch_accelerator("Ctrl", "Tab").as_deref(),
            Some("Ctrl+Tab")
        );
        assert_eq!(
            tab_switch_accelerator("Ctrl", "`").as_deref(),
            Some("Ctrl+Backquote")
        );
        assert_eq!(
            tab_switch_accelerator("Alt", "q").as_deref(),
            Some("Alt+KeyQ")
        );
        assert_eq!(tab_switch_accelerator("None", "F2").as_deref(), Some("F2"));
        assert_eq!(tab_switch_accelerator("None", ""), None);
    }

    #[test]
    fn conflicts_with_tab_switch_or_another_favorite() {
        let favs = vec!["Ctrl+Digit1".to_string(), String::new(), "F5".to_string()];
        assert_eq!(
            find_conflict("Ctrl+Tab", 1, &favs, Some("Ctrl+Tab")),
            Some(Conflict::TabSwitch)
        );
        assert_eq!(
            find_conflict("F5", 1, &favs, None),
            Some(Conflict::Favorite(2))
        );
        // Its own current shortcut is not a conflict.
        assert_eq!(find_conflict("Ctrl+Digit1", 0, &favs, None), None);
        assert_eq!(find_conflict("", 1, &favs, None), None);
    }
}
