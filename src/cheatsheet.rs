//! The cheat sheet: class and dungeon names in Japanese and Korean, so a
//! Korean player can read (and type) what Japanese players say. Static data
//! plus the search over it -- pure, host-tested.
//!
//! The lists are NOT yet the game's official names: they were assembled from
//! fan sites while the official ones were out of reach, and hold only pairs
//! whose two names plainly name the same thing. Check against the official
//! sites, then add the rest here (and clear [`SOURCE_NOTE`]).

/// Shown under the lists until the data is checked against the official sites.
pub const SOURCE_NOTE: &str = "공식 사이트 확인 전 · 팬 사이트 기준 일부 목록";

/// One thing, named in both languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub ja: &'static str,
    pub ko: &'static str,
}

const fn entry(ja: &'static str, ko: &'static str) -> Entry {
    Entry { ja, ko }
}

pub const CLASSES: &[Entry] = &[
    entry("ストームブレイド", "스톰 블레이드"),
    entry("フロストメイジ", "프로스트 메이지"),
    entry("ヘヴィガーディアン", "헤비 가디언"),
];

pub const DUNGEONS: &[Entry] = &[
    entry("ゴブリンの巣窟", "고블린 소굴"),
    entry("巨塔の遺跡", "거탑 유적"),
    entry("ティナ・精神領域", "티나·정신 세계"),
    entry("巨竜の爪痕", "거룡의 발톱"),
    entry("極限空間", "극한 공간"),
];

/// A list on the cheat sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Classes,
    Dungeons,
}

impl Section {
    pub const ALL: [Section; 2] = [Section::Classes, Section::Dungeons];

    pub fn label(self) -> &'static str {
        match self {
            Section::Classes => "직업",
            Section::Dungeons => "던전",
        }
    }

    pub fn entries(self) -> &'static [Entry] {
        match self {
            Section::Classes => CLASSES,
            Section::Dungeons => DUNGEONS,
        }
    }
}

/// What a search compares: lower case, without spaces or the dots that
/// separate parts of a name ("스톰 블레이드" is found by "스톰블레이드").
fn key(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '・' | '·' | '･' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The entries of `section` whose Japanese or Korean name contains `query`;
/// all of them for an empty query.
pub fn search(section: Section, query: &str) -> Vec<Entry> {
    let query = key(query);
    section
        .entries()
        .iter()
        .copied()
        .filter(|e| query.is_empty() || key(e.ja).contains(&query) || key(e.ko).contains(&query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_types::contains_japanese;

    fn all() -> impl Iterator<Item = Entry> {
        Section::ALL
            .into_iter()
            .flat_map(|s| s.entries().iter().copied())
    }

    #[test]
    fn every_entry_has_a_japanese_and_a_korean_name() {
        for e in all() {
            assert!(contains_japanese(e.ja), "{e:?}");
            assert!(
                e.ko.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)),
                "{e:?}"
            );
        }
    }

    #[test]
    fn no_name_is_listed_twice_in_a_section() {
        for section in Section::ALL {
            let entries = section.entries();
            for (i, a) in entries.iter().enumerate() {
                for b in &entries[i + 1..] {
                    assert!(
                        key(a.ja) != key(b.ja) && key(a.ko) != key(b.ko),
                        "{a:?} {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_empty_search_lists_everything() {
        for section in Section::ALL {
            assert_eq!(search(section, "").len(), section.entries().len());
            assert_eq!(search(section, "  ").len(), section.entries().len());
        }
    }

    #[test]
    fn a_search_matches_either_language() {
        let by_ja = search(Section::Classes, "ストーム");
        let by_ko = search(Section::Classes, "스톰");
        assert_eq!(by_ja, by_ko);
        assert_eq!(by_ja.len(), 1);
        assert!(search(Section::Classes, "ストーム")[0].ko.contains("스톰"));
    }

    #[test]
    fn a_search_ignores_spaces_dots_and_case() {
        assert_eq!(search(Section::Classes, "스톰블레이드").len(), 1);
        assert_eq!(
            search(Section::Dungeons, "티나 정신").len(),
            1,
            "a dot in the name"
        );
        assert_eq!(search(Section::Dungeons, "티나정신세계").len(), 1);
        assert_eq!(search(Section::Dungeons, "ティナ精神").len(), 1);
    }

    #[test]
    fn a_search_only_looks_in_its_own_section() {
        assert!(search(Section::Classes, "고블린").is_empty());
        assert_eq!(search(Section::Dungeons, "고블린").len(), 1);
    }

    #[test]
    fn the_sections_are_named() {
        assert_eq!(Section::ALL.map(|s| s.label()), ["직업", "던전"]);
    }
}
