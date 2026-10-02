//! What a chat row shows of the translation, and how the title bar names it.
//! Whether the translator runs at all is `use_translation` (settings);
//! `translation_view` is only what the rows do with what it produces.

use crate::ui_types::TranslationView;

/// The view that is really in force: with the translator switched off there
/// is nothing to show, so "on" reads as "off". The saved choice is kept, and
/// comes back when the translator does.
pub fn effective(view: TranslationView, use_translation: bool) -> TranslationView {
    if view == TranslationView::On && !use_translation {
        TranslationView::Off
    } else {
        view
    }
}

/// The title-bar badge's text.
pub fn label(view: TranslationView) -> &'static str {
    match view {
        TranslationView::On => "번역 ON",
        TranslationView::Off => "번역 OFF",
        TranslationView::Study => "공부 모드",
    }
}

/// The badge's colours; every view is clickable (it opens the picker).
pub fn pill_class(view: TranslationView) -> &'static str {
    match view {
        TranslationView::On => {
            "bg-success/10 text-success border-success/20 cursor-pointer hover:bg-success/20"
        }
        TranslationView::Off => {
            "bg-base-content/5 text-base-content/50 border-base-content/10 cursor-pointer hover:bg-base-content/10"
        }
        TranslationView::Study => {
            "bg-info/10 text-info border-info/20 cursor-pointer hover:bg-info/20"
        }
    }
}

/// What the picker says under each choice.
pub fn hint(view: TranslationView) -> &'static str {
    match view {
        TranslationView::On => "번역을 먼저, 원문은 아래에 표시",
        TranslationView::Off => "원문만 표시",
        TranslationView::Study => "원문에 후리가나, 마우스를 올리면 번역 표시",
    }
}

/// Whether rows put the translation first (the "on" view).
pub fn shows_translation(view: TranslationView) -> bool {
    view == TranslationView::On
}

/// Display classes of a line that appears while the pointer is over its row
/// (the study view's translation). Exactly one bare `display` utility, as in
/// `chat_view::compact_original_class`.
pub const HOVER_ONLY: &str = "hidden group-hover:block";

#[cfg(test)]
mod tests {
    use super::*;
    use TranslationView::*;

    #[test]
    fn with_the_translator_off_the_on_view_reads_as_off() {
        for (view, use_translation, want) in [
            (On, true, On),
            (On, false, Off),
            (Off, true, Off),
            (Off, false, Off),
            (Study, true, Study),
            // Furigana needs no translator.
            (Study, false, Study),
        ] {
            assert_eq!(
                effective(view, use_translation),
                want,
                "{view:?} {use_translation}"
            );
        }
    }

    #[test]
    fn each_view_has_its_own_korean_label_colours_and_hint() {
        let all: Vec<_> = TranslationView::ALL.to_vec();
        assert_eq!(all, [On, Off, Study]);
        assert_eq!(
            all.iter().map(|v| label(*v)).collect::<Vec<_>>(),
            ["번역 ON", "번역 OFF", "공부 모드"]
        );
        for field in [
            all.iter().map(|v| pill_class(*v)).collect::<Vec<_>>(),
            all.iter().map(|v| hint(*v)).collect::<Vec<_>>(),
        ] {
            let mut unique = field.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), 3, "{field:?}");
            assert!(field.iter().all(|s| !s.is_empty()));
        }
        assert!(all
            .iter()
            .all(|v| pill_class(*v).contains("cursor-pointer")));
    }

    #[test]
    fn only_the_on_view_puts_the_translation_first() {
        assert!(shows_translation(On));
        assert!(!shows_translation(Off));
        assert!(!shows_translation(Study));
    }

    #[test]
    fn a_hover_only_line_has_no_competing_display_class() {
        let display: Vec<&str> = HOVER_ONLY
            .split_whitespace()
            .filter(|c| matches!(*c, "inline" | "hidden" | "block" | "flex"))
            .collect();
        assert_eq!(display, ["hidden"]);
        assert!(HOVER_ONLY.contains("group-hover:block"));
    }
}
