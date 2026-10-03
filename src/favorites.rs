//! Favorite-message list edits that need no UI -- pure, host-tested.
//!
//! Favorites are one flat list (the backend's shortcuts need no tabs); each
//! carries the id of the tab it is filed under. The default tab (id 0) is not
//! in the tab list, comes first on screen and cannot be removed. A tab is its
//! id, not its name: tabs may share a name, and a later rename or reorder
//! touches no message.

use crate::ui_types::{default_favorite_messages, FavoriteMessage, FavoriteTab};
use resonance_types::DEFAULT_FAVORITE_TAB;

/// The id of the default tab (`FavoriteMessage::tab` when unset).
pub const DEFAULT_TAB: u32 = DEFAULT_FAVORITE_TAB;
/// What the default tab is called on screen.
pub const DEFAULT_TAB_LABEL: &str = "기본";
/// Longest tab name, in characters: keeps the tab strip readable.
pub const MAX_TAB_NAME_CHARS: usize = 12;

/// Why a tab name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabError {
    Empty,
    TooLong,
}

impl TabError {
    pub fn message(self) -> &'static str {
        match self {
            TabError::Empty => "탭 이름을 입력하세요.",
            TabError::TooLong => "탭 이름이 너무 깁니다. (최대 12자)",
        }
    }
}

/// The name on screen for tab `id`; the default tab, and an id no tab has
/// (its messages are filed in the default tab), read as the default's.
pub fn tab_label(tabs: &[FavoriteTab], id: u32) -> &str {
    tabs.iter()
        .find(|t| t.id == id)
        .map_or(DEFAULT_TAB_LABEL, |t| t.name.as_str())
}

/// The tab to show when the window has `wanted` open: itself, or the default
/// tab when another window deleted it meanwhile.
pub fn shown_tab(tabs: &[FavoriteTab], wanted: u32) -> u32 {
    if wanted == DEFAULT_TAB || tabs.iter().any(|t| t.id == wanted) {
        wanted
    } else {
        DEFAULT_TAB
    }
}

/// The id the next tab gets: past every id in use, never the default's.
fn next_id(tabs: &[FavoriteTab]) -> u32 {
    tabs.iter()
        .map(|t| t.id)
        .max()
        .unwrap_or(DEFAULT_TAB)
        .max(DEFAULT_TAB)
        + 1
}

/// Adds a tab named `name` (trimmed) at the end; returns the id it got. The
/// name need not be new.
pub fn add_tab(tabs: &mut Vec<FavoriteTab>, name: &str) -> Result<u32, TabError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(TabError::Empty);
    }
    if name.chars().count() > MAX_TAB_NAME_CHARS {
        return Err(TabError::TooLong);
    }
    let id = next_id(tabs);
    tabs.push(FavoriteTab {
        id,
        name: name.to_string(),
    });
    Ok(id)
}

/// Removes tab `id` and every message filed under it; returns how many
/// messages went. The default tab (and an id nobody has) removes nothing.
pub fn delete_tab(tabs: &mut Vec<FavoriteTab>, list: &mut Vec<FavoriteMessage>, id: u32) -> usize {
    let Some(at) = tabs.iter().position(|t| t.id == id) else {
        return 0;
    };
    tabs.remove(at);
    let before = list.len();
    list.retain(|f| f.tab != id);
    before - list.len()
}

/// What deleting tab `id` would take: (messages, messages with a shortcut).
pub fn tab_summary(list: &[FavoriteMessage], id: u32) -> (usize, usize) {
    let own = list.iter().filter(|f| f.tab == id);
    (
        own.clone().count(),
        own.filter(|f| !f.shortcut.is_empty()).count(),
    )
}

/// Files the default messages under tab `tab`, skipping lines it already has;
/// returns how many were added.
pub fn fill_with_defaults(list: &mut Vec<FavoriteMessage>, tab: u32) -> usize {
    let mut added = 0;
    for mut fav in default_favorite_messages() {
        if list.iter().any(|f| f.tab == tab && f.text == fav.text) {
            continue;
        }
        fav.tab = tab;
        list.push(fav);
        added += 1;
    }
    added
}

/// Saved tabs as loaded: a name is trimmed, cut to the longest allowed, and a
/// blank one becomes "탭 N" (N = the tab's place, from 1); a tab whose id is the
/// default tab's or one an earlier tab has gets a fresh id. No tab is lost,
/// and what is already clean stays as it is.
pub fn clean_tabs(saved: Vec<FavoriteTab>) -> Vec<FavoriteTab> {
    let mut next = next_id(&saved);
    let mut taken = vec![DEFAULT_TAB];
    let mut tabs = Vec::with_capacity(saved.len());
    for (place, tab) in saved.into_iter().enumerate() {
        let id = if taken.contains(&tab.id) {
            next += 1;
            next - 1
        } else {
            tab.id
        };
        taken.push(id);
        let name = tab.name.trim();
        let name = if name.is_empty() {
            format!("탭 {}", place + 1)
        } else {
            name.chars().take(MAX_TAB_NAME_CHARS).collect()
        };
        tabs.push(FavoriteTab { id, name });
    }
    tabs
}

/// Moves messages whose tab is not in `tabs` into the default tab.
pub fn normalize(list: &mut [FavoriteMessage], tabs: &[FavoriteTab]) {
    for fav in list {
        if fav.tab != DEFAULT_TAB && !tabs.iter().any(|t| t.id == fav.tab) {
            fav.tab = DEFAULT_TAB;
        }
    }
}

/// Where message `index` sits: its tab's on-screen name and its 1-based
/// place among that tab's messages.
pub fn locate(
    list: &[FavoriteMessage],
    tabs: &[FavoriteTab],
    index: usize,
) -> Option<(String, usize)> {
    let tab = list.get(index)?.tab;
    let place = list[..=index].iter().filter(|f| f.tab == tab).count();
    Some((tab_label(tabs, tab).to_string(), place))
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

/// One of a favorite's boxes in the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// The message (Japanese): what is pasted into the game.
    Text,
    /// The reminder under it (its meaning in Korean); never sent.
    Note,
    /// The global shortcut, as an accelerator ("Alt+F1"); empty is none.
    Shortcut,
}

/// Why an edit was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    /// A message needs text.
    EmptyText,
}

impl EditError {
    pub fn message(self) -> &'static str {
        match self {
            EditError::EmptyText => "메시지는 비워 둘 수 없습니다.",
        }
    }
}

/// Sets one box of the message at `index` (text and note are trimmed).
/// `Ok(true)` when it changed -- only then is there something to save;
/// `Ok(false)` when it was already that, or the row is gone. A refused edit
/// changes nothing.
pub fn set_field(
    list: &mut [FavoriteMessage],
    index: usize,
    field: Field,
    value: &str,
) -> Result<bool, EditError> {
    let Some(fav) = list.get_mut(index) else {
        return Ok(false);
    };
    let value = match field {
        Field::Shortcut => value,
        Field::Text | Field::Note => value.trim(),
    };
    if field == Field::Text && value.is_empty() {
        return Err(EditError::EmptyText);
    }
    let slot = match field {
        Field::Text => &mut fav.text,
        Field::Note => &mut fav.note,
        Field::Shortcut => &mut fav.shortcut,
    };
    if slot == value {
        return Ok(false);
    }
    *slot = value.to_string();
    Ok(true)
}

/// The places in `list` of the messages filed under `tab`: a row's key in the
/// table is its place in the whole list.
pub fn indices_in_tab(list: &[FavoriteMessage], tab: u32) -> Vec<usize> {
    list.iter()
        .enumerate()
        .filter(|(_, f)| f.tab == tab)
        .map(|(i, _)| i)
        .collect()
}

/// A message for tab `tab` from the boxes of the table's blank row; refused
/// without text.
pub fn new_message(
    tab: u32,
    text: &str,
    note: &str,
    shortcut: &str,
) -> Result<FavoriteMessage, EditError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(EditError::EmptyText);
    }
    Ok(FavoriteMessage {
        text: text.to_string(),
        note: note.trim().to_string(),
        shortcut: shortcut.to_string(),
        tab,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- the table's boxes ---

    #[test]
    fn a_box_is_set_trimmed_and_reports_whether_it_changed() {
        let mut list = vec![fav("こんにちは", 0), fav("またね", 0)];
        assert_eq!(
            set_field(&mut list, 1, Field::Text, "  さよなら "),
            Ok(true)
        );
        assert_eq!(list[1].text, "さよなら");
        assert_eq!(set_field(&mut list, 1, Field::Text, "さよなら"), Ok(false));
        assert_eq!(set_field(&mut list, 0, Field::Note, " 안녕 "), Ok(true));
        assert_eq!(list[0].note, "안녕");
        assert_eq!(list[0].text, "こんにちは", "the other boxes stay");
    }

    #[test]
    fn the_message_box_cannot_be_emptied_but_the_note_can() {
        let mut list = vec![FavoriteMessage {
            text: "こんにちは".into(),
            note: "안녕".into(),
            ..Default::default()
        }];
        assert_eq!(
            set_field(&mut list, 0, Field::Text, "   "),
            Err(EditError::EmptyText)
        );
        assert_eq!(list[0].text, "こんにちは", "a refused edit changes nothing");
        assert_eq!(set_field(&mut list, 0, Field::Note, ""), Ok(true));
        assert!(list[0].note.is_empty());
    }

    #[test]
    fn a_shortcut_is_set_and_cleared_as_given() {
        let mut list = vec![fav("a", 0)];
        assert_eq!(set_field(&mut list, 0, Field::Shortcut, "Alt+F1"), Ok(true));
        assert_eq!(list[0].shortcut, "Alt+F1");
        assert_eq!(set_field(&mut list, 0, Field::Shortcut, ""), Ok(true));
        assert!(list[0].shortcut.is_empty());
        assert_eq!(set_field(&mut list, 0, Field::Shortcut, ""), Ok(false));
    }

    #[test]
    fn a_box_of_a_row_that_is_gone_is_ignored() {
        let mut list = vec![fav("a", 0)];
        assert_eq!(set_field(&mut list, 5, Field::Text, "b"), Ok(false));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn a_tabs_rows_are_found_by_their_place_in_the_whole_list() {
        let list = vec![fav("a", 0), fav("b", 3), fav("c", 0), fav("d", 3)];
        assert_eq!(indices_in_tab(&list, 0), [0, 2]);
        assert_eq!(indices_in_tab(&list, 3), [1, 3]);
        assert!(indices_in_tab(&list, 9).is_empty());
    }

    #[test]
    fn a_new_row_becomes_a_message_in_its_tab_once_it_has_text() {
        let made = new_message(2, "  よろしく ", " 잘 부탁 ", "Alt+F2").unwrap();
        assert_eq!(made.text, "よろしく");
        assert_eq!(made.note, "잘 부탁");
        assert_eq!(made.shortcut, "Alt+F2");
        assert_eq!(made.tab, 2);
        assert_eq!(
            new_message(DEFAULT_TAB, "  ", "메모", ""),
            Err(EditError::EmptyText)
        );
    }

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

    fn fav(text: &str, tab: u32) -> FavoriteMessage {
        FavoriteMessage {
            text: text.into(),
            tab,
            ..Default::default()
        }
    }

    fn tab(id: u32, name: &str) -> FavoriteTab {
        FavoriteTab {
            id,
            name: name.into(),
        }
    }

    fn ids(tabs: &[FavoriteTab]) -> Vec<u32> {
        tabs.iter().map(|t| t.id).collect()
    }

    fn names(tabs: &[FavoriteTab]) -> Vec<&str> {
        tabs.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn a_chat_message_is_filed_in_the_default_tab_and_only_deduplicated_there() {
        let mut list = vec![fav("またね！", 1)];
        assert!(add_from_chat(&mut list, "またね！", None));
        assert_eq!(list[1].tab, DEFAULT_TAB);
        assert!(!add_from_chat(&mut list, "またね！", None));
    }

    #[test]
    fn a_new_tab_is_trimmed_listed_and_gets_the_next_id() {
        let mut tabs = vec![tab(1, "레이드")];
        assert_eq!(add_tab(&mut tabs, "  던전 "), Ok(2));
        assert_eq!(names(&tabs), ["레이드", "던전"]);
        assert_eq!(ids(&tabs), [1, 2]);
        let mut none = Vec::new();
        assert_eq!(
            add_tab(&mut none, "처음"),
            Ok(1),
            "id 0 is the default tab's"
        );
    }

    #[test]
    fn a_tab_name_must_be_short_and_not_empty() {
        let mut tabs = vec![tab(1, "레이드")];
        assert_eq!(add_tab(&mut tabs, "   "), Err(TabError::Empty));
        let long = "가".repeat(MAX_TAB_NAME_CHARS + 1);
        assert_eq!(add_tab(&mut tabs, &long), Err(TabError::TooLong));
        let just_fits = "가".repeat(MAX_TAB_NAME_CHARS);
        assert!(add_tab(&mut tabs, &just_fits).is_ok());
        assert_eq!(tabs.len(), 2, "failed adds leave the list alone");
    }

    #[test]
    fn tabs_may_share_a_name_even_the_default_tabs() {
        let mut tabs = vec![tab(1, "Raid")];
        assert_eq!(add_tab(&mut tabs, "Raid"), Ok(2));
        assert_eq!(add_tab(&mut tabs, "raid"), Ok(3));
        assert_eq!(add_tab(&mut tabs, DEFAULT_TAB_LABEL), Ok(4));
        assert_eq!(ids(&tabs), [1, 2, 3, 4]);
    }

    #[test]
    fn a_new_tab_never_takes_the_id_of_a_tab_that_is_still_there() {
        let mut tabs = vec![tab(1, "a"), tab(2, "b"), tab(3, "c")];
        let mut list = Vec::new();
        delete_tab(&mut tabs, &mut list, 2);
        assert_eq!(add_tab(&mut tabs, "d"), Ok(4));
        assert_eq!(ids(&tabs), [1, 3, 4]);
    }

    #[test]
    fn deleting_a_tab_deletes_its_messages_and_only_those() {
        let mut tabs = vec![tab(1, "레이드"), tab(2, "던전")];
        let mut list = vec![fav("a", 0), fav("b", 1), fav("c", 2), fav("d", 1)];
        assert_eq!(delete_tab(&mut tabs, &mut list, 1), 2);
        assert_eq!(names(&tabs), ["던전"]);
        let left: Vec<_> = list.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(left, ["a", "c"]);
    }

    #[test]
    fn deleting_one_of_two_tabs_with_the_same_name_keeps_the_other() {
        let mut tabs = vec![tab(1, "레이드"), tab(2, "레이드")];
        let mut list = vec![fav("a", 1), fav("b", 2)];
        assert_eq!(delete_tab(&mut tabs, &mut list, 2), 1);
        assert_eq!(ids(&tabs), [1]);
        assert_eq!(list, [fav("a", 1)]);
    }

    #[test]
    fn the_default_tab_cannot_be_deleted() {
        let mut tabs = vec![tab(1, "레이드")];
        let mut list = vec![fav("a", 0)];
        assert_eq!(delete_tab(&mut tabs, &mut list, DEFAULT_TAB), 0);
        assert_eq!(delete_tab(&mut tabs, &mut list, 9), 0);
        assert_eq!((tabs.len(), list.len()), (1, 1));
    }

    #[test]
    fn the_delete_warning_counts_messages_and_shortcuts() {
        let mut keyed = fav("b", 1);
        keyed.shortcut = "Ctrl+Digit1".into();
        let list = vec![fav("a", 0), keyed, fav("c", 1)];
        assert_eq!(tab_summary(&list, 1), (2, 1));
        assert_eq!(tab_summary(&list, 9), (0, 0));
    }

    #[test]
    fn a_tab_can_be_filled_with_the_default_messages_once() {
        let mut list = vec![fav("こんにちは！", 1)];
        let added = fill_with_defaults(&mut list, 1);
        assert_eq!(
            added,
            default_favorite_messages().len() - 1,
            "one is already there"
        );
        assert!(list.iter().all(|f| f.tab == 1));
        assert_eq!(fill_with_defaults(&mut list, 1), 0);
        // The same lines in another tab are separate entries.
        assert_eq!(
            fill_with_defaults(&mut list, 2),
            default_favorite_messages().len()
        );
    }

    #[test]
    fn saved_tabs_are_cleaned_when_loaded() {
        let long = "가".repeat(MAX_TAB_NAME_CHARS + 5);
        let cleaned = clean_tabs(vec![
            tab(1, " 레이드 "),
            tab(2, ""),
            tab(3, "   "),
            tab(4, &long),
            tab(5, "레이드"),
        ]);
        assert_eq!(
            names(&cleaned),
            [
                "레이드",
                "탭 2",
                "탭 3",
                &"가".repeat(MAX_TAB_NAME_CHARS),
                "레이드"
            ],
            "a blank name becomes 탭 N (its place), a long one is cut, repeats stay"
        );
        assert_eq!(ids(&cleaned), [1, 2, 3, 4, 5], "ids are kept");
    }

    #[test]
    fn a_tab_with_the_default_id_or_a_repeated_id_gets_a_fresh_one() {
        let cleaned = clean_tabs(vec![tab(0, "a"), tab(2, "b"), tab(2, "c")]);
        assert_eq!(names(&cleaned), ["a", "b", "c"], "no tab is lost");
        assert_eq!(ids(&cleaned), [3, 2, 4]);
        assert_eq!(
            clean_tabs(cleaned.clone()),
            cleaned,
            "cleaning twice changes nothing"
        );
    }

    #[test]
    fn a_message_in_a_tab_that_is_gone_lands_in_the_default_tab() {
        let tabs = vec![tab(1, "레이드")];
        let mut list = vec![fav("a", 1), fav("b", 7), fav("c", 0)];
        normalize(&mut list, &tabs);
        let filed: Vec<_> = list.iter().map(|f| f.tab).collect();
        assert_eq!(filed, [1, 0, 0]);
    }

    #[test]
    fn a_message_is_found_by_its_place_in_its_own_tab() {
        let tabs = vec![tab(4, "레이드")];
        let list = vec![fav("a", 0), fav("b", 4), fav("c", 0), fav("d", 4)];
        assert_eq!(locate(&list, &tabs, 2), Some(("기본".to_string(), 2)));
        assert_eq!(locate(&list, &tabs, 3), Some(("레이드".to_string(), 2)));
        assert_eq!(locate(&list, &tabs, 9), None);
    }

    #[test]
    fn a_tab_that_is_gone_while_open_shows_the_default_tab() {
        let tabs = vec![tab(4, "레이드")];
        assert_eq!(shown_tab(&tabs, 4), 4);
        assert_eq!(shown_tab(&tabs, DEFAULT_TAB), DEFAULT_TAB);
        assert_eq!(
            shown_tab(&tabs, 9),
            DEFAULT_TAB,
            "deleted in another window"
        );
    }

    #[test]
    fn a_tab_is_shown_by_its_name_and_the_default_tab_as_basic() {
        let tabs = vec![tab(4, "레이드")];
        assert_eq!(tab_label(&tabs, DEFAULT_TAB), "기본");
        assert_eq!(tab_label(&tabs, 4), "레이드");
        assert_eq!(
            tab_label(&tabs, 9),
            "기본",
            "a tab that is gone reads as the default"
        );
    }
}
