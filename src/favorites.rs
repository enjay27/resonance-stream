//! Favorite-message list edits that need no UI -- pure, host-tested.

use crate::ui_types::FavoriteMessage;

/// Add a chat message to the favorites: its text, with its translation (if
/// any) as the note. Nothing is added for empty text or text already in the
/// list; returns whether it was added.
pub fn add_from_chat(
    list: &mut Vec<FavoriteMessage>,
    text: &str,
    translated: Option<&str>,
) -> bool {
    let text = text.trim();
    if text.is_empty() || list.iter().any(|f| f.text.trim() == text) {
        return false;
    }
    list.push(FavoriteMessage {
        text: text.to_string(),
        note: translated.map(str::trim).unwrap_or_default().to_string(),
        shortcut: String::new(),
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_text_with_translation_as_note() {
        let mut list = Vec::new();
        assert!(add_from_chat(&mut list, " よろしく ", Some("잘 부탁해 ")));
        assert_eq!(list[0].text, "よろしく");
        assert_eq!(list[0].note, "잘 부탁해");
        assert!(list[0].shortcut.is_empty());
    }

    #[test]
    fn untranslated_message_gets_an_empty_note() {
        let mut list = Vec::new();
        assert!(add_from_chat(&mut list, "gg", None));
        assert!(list[0].note.is_empty());
    }

    #[test]
    fn skips_duplicates_and_empty_text() {
        let mut list = vec![FavoriteMessage {
            text: "またね！".into(),
            ..Default::default()
        }];
        assert!(!add_from_chat(&mut list, "またね！ ", None));
        assert!(!add_from_chat(&mut list, "   ", None));
        assert_eq!(list.len(), 1);
    }
}
