//! Favorite-message list edits that need no UI -- pure, host-tested.
//!
//! Favorites are one flat list (the backend's shortcuts need no tabs); each
//! carries the name of the tab it is filed under. The default tab has no name
//! of its own and cannot be removed.

use crate::ui_types::{default_favorite_messages, FavoriteMessage};

/// The stored name of the default tab (`FavoriteMessage::tab` when unset).
pub const DEFAULT_TAB: &str = "";
/// What the default tab is called on screen.
pub const DEFAULT_TAB_LABEL: &str = "기본";
/// Longest tab name, in characters: keeps the tab strip readable.
pub const MAX_TAB_NAME_CHARS: usize = 12;

/// Why a tab name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabError {
    Empty,
    TooLong,
    /// Already a tab, or the default tab's name.
    Taken,
}

impl TabError {
    pub fn message(self) -> &'static str {
        match self {
            TabError::Empty => "탭 이름을 입력하세요.",
            TabError::TooLong => "탭 이름이 너무 깁니다. (최대 12자)",
            TabError::Taken => "이미 있는 탭 이름입니다.",
        }
    }
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The name on screen for the stored name `tab`.
pub fn tab_label(tab: &str) -> &str {
    if tab == DEFAULT_TAB {
        DEFAULT_TAB_LABEL
    } else {
        tab
    }
}

/// Adds a tab named `name` (trimmed) at the end; returns the name it got.
pub fn add_tab(tabs: &mut Vec<String>, name: &str) -> Result<String, TabError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(TabError::Empty);
    }
    if name.chars().count() > MAX_TAB_NAME_CHARS {
        return Err(TabError::TooLong);
    }
    if same_name(name, DEFAULT_TAB_LABEL) || tabs.iter().any(|t| same_name(t, name)) {
        return Err(TabError::Taken);
    }
    tabs.push(name.to_string());
    Ok(name.to_string())
}

/// Removes tab `name` and every message filed under it; returns how many
/// messages went. The default tab (and a name nobody has) removes nothing.
pub fn delete_tab(tabs: &mut Vec<String>, list: &mut Vec<FavoriteMessage>, name: &str) -> usize {
    let Some(at) = tabs.iter().position(|t| t == name) else {
        return 0;
    };
    tabs.remove(at);
    let before = list.len();
    list.retain(|f| f.tab != name);
    before - list.len()
}

/// What deleting tab `name` would take: (messages, messages with a shortcut).
pub fn tab_summary(list: &[FavoriteMessage], name: &str) -> (usize, usize) {
    let own = list.iter().filter(|f| f.tab == name);
    (
        own.clone().count(),
        own.filter(|f| !f.shortcut.is_empty()).count(),
    )
}

/// Files the default messages under `tab`, skipping lines it already has;
/// returns how many were added.
pub fn fill_with_defaults(list: &mut Vec<FavoriteMessage>, tab: &str) -> usize {
    let mut added = 0;
    for mut fav in default_favorite_messages() {
        if list.iter().any(|f| f.tab == tab && f.text == fav.text) {
            continue;
        }
        fav.tab = tab.to_string();
        list.push(fav);
        added += 1;
    }
    added
}

/// Saved tab names as loaded: trimmed, no blanks, no repeats, none called
/// like the default tab.
pub fn clean_tabs(saved: Vec<String>) -> Vec<String> {
    let mut tabs = Vec::new();
    for name in saved {
        let _ = add_tab(&mut tabs, &name);
    }
    tabs
}

/// Moves messages whose tab is not in `tabs` into the default tab.
pub fn normalize(list: &mut [FavoriteMessage], tabs: &[String]) {
    for fav in list {
        if !tabs.contains(&fav.tab) {
            fav.tab = DEFAULT_TAB.to_string();
        }
    }
}

/// Where message `index` sits: its tab's on-screen name and its 1-based
/// place among that tab's messages.
pub fn locate(list: &[FavoriteMessage], index: usize) -> Option<(String, usize)> {
    let tab = &list.get(index)?.tab;
    let place = list[..=index].iter().filter(|f| &f.tab == tab).count();
    Some((tab_label(tab).to_string(), place))
}

/// Add a chat message to the favorites (default tab): its text, with its
/// translation (if any) as the note. Nothing is added for empty text or text
/// already in that tab; returns whether it was added.
pub fn add_from_chat(
    list: &mut Vec<FavoriteMessage>,
    text: &str,
    translated: Option<&str>,
) -> bool {
    let text = text.trim();
    if text.is_empty()
        || list
            .iter()
            .any(|f| f.tab == DEFAULT_TAB && f.text.trim() == text)
    {
        return false;
    }
    list.push(FavoriteMessage {
        text: text.to_string(),
        note: translated.map(str::trim).unwrap_or_default().to_string(),
        ..Default::default()
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

    fn fav(text: &str, tab: &str) -> FavoriteMessage {
        FavoriteMessage {
            text: text.into(),
            tab: tab.into(),
            ..Default::default()
        }
    }

    fn names(tabs: &[&str]) -> Vec<String> {
        tabs.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn a_chat_message_is_filed_in_the_default_tab_and_only_deduplicated_there() {
        let mut list = vec![fav("またね！", "레이드")];
        assert!(add_from_chat(&mut list, "またね！", None));
        assert_eq!(list[1].tab, DEFAULT_TAB);
        assert!(!add_from_chat(&mut list, "またね！", None));
    }

    #[test]
    fn a_new_tab_is_trimmed_and_listed() {
        let mut tabs = names(&["레이드"]);
        assert_eq!(add_tab(&mut tabs, "  던전 "), Ok("던전".to_string()));
        assert_eq!(tabs, ["레이드", "던전"]);
    }

    #[test]
    fn a_tab_name_must_be_new_short_and_not_empty() {
        let mut tabs = names(&["레이드"]);
        assert_eq!(add_tab(&mut tabs, "   "), Err(TabError::Empty));
        assert_eq!(add_tab(&mut tabs, "레이드"), Err(TabError::Taken));
        assert_eq!(add_tab(&mut tabs, DEFAULT_TAB_LABEL), Err(TabError::Taken));
        let long = "가".repeat(MAX_TAB_NAME_CHARS + 1);
        assert_eq!(add_tab(&mut tabs, &long), Err(TabError::TooLong));
        let just_fits = "가".repeat(MAX_TAB_NAME_CHARS);
        assert!(add_tab(&mut tabs, &just_fits).is_ok());
        assert_eq!(tabs.len(), 2, "failed adds leave the list alone");
    }

    #[test]
    fn tab_names_are_compared_without_regard_to_case() {
        let mut tabs = names(&["Raid"]);
        assert_eq!(add_tab(&mut tabs, "raid"), Err(TabError::Taken));
    }

    #[test]
    fn deleting_a_tab_deletes_its_messages_and_only_those() {
        let mut tabs = names(&["레이드", "던전"]);
        let mut list = vec![
            fav("a", ""),
            fav("b", "레이드"),
            fav("c", "던전"),
            fav("d", "레이드"),
        ];
        assert_eq!(delete_tab(&mut tabs, &mut list, "레이드"), 2);
        assert_eq!(tabs, ["던전"]);
        let left: Vec<_> = list.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(left, ["a", "c"]);
    }

    #[test]
    fn the_default_tab_cannot_be_deleted() {
        let mut tabs = names(&["레이드"]);
        let mut list = vec![fav("a", "")];
        assert_eq!(delete_tab(&mut tabs, &mut list, DEFAULT_TAB), 0);
        assert_eq!(delete_tab(&mut tabs, &mut list, "nope"), 0);
        assert_eq!((tabs.len(), list.len()), (1, 1));
    }

    #[test]
    fn the_delete_warning_counts_messages_and_shortcuts() {
        let mut keyed = fav("b", "레이드");
        keyed.shortcut = "Ctrl+Digit1".into();
        let list = vec![fav("a", ""), keyed, fav("c", "레이드")];
        assert_eq!(tab_summary(&list, "레이드"), (2, 1));
        assert_eq!(tab_summary(&list, "없는 탭"), (0, 0));
    }

    #[test]
    fn a_tab_can_be_filled_with_the_default_messages_once() {
        let mut list = vec![fav("こんにちは！", "레이드")];
        let added = fill_with_defaults(&mut list, "레이드");
        assert_eq!(
            added,
            default_favorite_messages().len() - 1,
            "one is already there"
        );
        assert!(list.iter().all(|f| f.tab == "레이드"));
        assert_eq!(fill_with_defaults(&mut list, "레이드"), 0);
        // The same lines in another tab are separate entries.
        assert_eq!(
            fill_with_defaults(&mut list, "던전"),
            default_favorite_messages().len()
        );
    }

    #[test]
    fn saved_tabs_are_cleaned_when_loaded() {
        let saved = names(&[
            " 레이드 ",
            "",
            "레이드",
            DEFAULT_TAB_LABEL,
            "raid",
            "RAID",
            "던전",
        ]);
        assert_eq!(clean_tabs(saved), ["레이드", "raid", "던전"]);
    }

    #[test]
    fn a_message_in_a_tab_that_is_gone_lands_in_the_default_tab() {
        let tabs = names(&["레이드"]);
        let mut list = vec![fav("a", "레이드"), fav("b", "사라진 탭"), fav("c", "")];
        normalize(&mut list, &tabs);
        let filed: Vec<_> = list.iter().map(|f| f.tab.as_str()).collect();
        assert_eq!(filed, ["레이드", "", ""]);
    }

    #[test]
    fn a_message_is_found_by_its_place_in_its_own_tab() {
        let list = vec![
            fav("a", ""),
            fav("b", "레이드"),
            fav("c", ""),
            fav("d", "레이드"),
        ];
        assert_eq!(locate(&list, 2), Some(("기본".to_string(), 2)));
        assert_eq!(locate(&list, 3), Some(("레이드".to_string(), 2)));
        assert_eq!(locate(&list, 9), None);
    }
}
