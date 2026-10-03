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

/// One thing, named in both languages. A class carries its two specializations
/// ("trees"); a tree or a dungeon has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub ja: &'static str,
    pub ko: &'static str,
    /// A second Japanese name for the same thing (empty for none): a tree the
    /// players write both as kanji and as kana. Searched and shown, not copied.
    pub also: &'static str,
    pub trees: &'static [Entry],
}

const fn entry(ja: &'static str, ko: &'static str) -> Entry {
    Entry {
        ja,
        ko,
        also: "",
        trees: &[],
    }
}

/// An entry with a second Japanese name.
const fn aka(ja: &'static str, also: &'static str, ko: &'static str) -> Entry {
    Entry {
        ja,
        ko,
        also,
        trees: &[],
    }
}

/// A class and its two trees.
const fn class(ja: &'static str, ko: &'static str, trees: &'static [Entry]) -> Entry {
    Entry {
        ja,
        ko,
        also: "",
        trees,
    }
}

pub const CLASSES: &[Entry] = &[
    class(
        "ツインストライカー",
        "트윈 스트라이커",
        &[entry("双炎", "무상"), entry("炎舞", "적홍")],
    ),
    class(
        "ストームブレイド",
        "스톰 블레이드",
        &[entry("雷刃", "발도"), entry("月影", "월광")],
    ),
    class(
        "ゲイルランサー",
        "윈드 나이트",
        &[entry("烈風", "질풍"), entry("乱風", "난무")],
    ),
    class(
        "フロストメイジ",
        "프로스트 메이지",
        &[entry("氷牙", "얼음창"), entry("霜天", "얼음빔")],
    ),
    class(
        "ディバインアーチャー",
        "디바인 아처",
        &[entry("狼弓", "늑대활"), entry("鷹弓", "매활")],
    ),
    class(
        "ヘヴィガーディアン",
        "헤비 가디언",
        &[entry("剛身", "방패"), entry("剛守", "가드")],
    ),
    class(
        "シールドファイター",
        "실드 나이트",
        &[entry("光砕", "방패"), entry("光盾", "광휘")],
    ),
    class(
        "ヴァーダントオラクル",
        "실반 오라클",
        &[aka("威咲", "イサキ", "심판"), entry("森癒", "치유")],
    ),
    class(
        "ビートパフォーマー",
        "비트 퍼포머",
        &[entry("狂音", "음파"), entry("響奏", "협주")],
    ),
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

/// One search result: an entry and the trees to show under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub entry: Entry,
    pub trees: Vec<Entry>,
}

/// Whether `query` (already a [`key`]) is in the entry's own names.
fn names_match(e: &Entry, query: &str) -> bool {
    key(e.ja).contains(query) || key(e.ko).contains(query) || key(e.also).contains(query)
}

/// The entries of `section` that `query` finds, in either language; all of
/// them for an empty query. A class found by its own name shows both its trees;
/// one found only through a tree shows just the trees that matched.
pub fn search(section: Section, query: &str) -> Vec<Hit> {
    let query = key(query);
    section
        .entries()
        .iter()
        .filter_map(|&entry| {
            if query.is_empty() || names_match(&entry, &query) {
                return Some(Hit {
                    entry,
                    trees: entry.trees.to_vec(),
                });
            }
            let trees: Vec<Entry> = entry
                .trees
                .iter()
                .copied()
                .filter(|t| names_match(t, &query))
                .collect();
            (!trees.is_empty()).then_some(Hit { entry, trees })
        })
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

    fn has_korean(text: &str) -> bool {
        text.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c))
    }

    /// The names of a section's hits, class names only.
    fn names(hits: &[Hit]) -> Vec<&'static str> {
        hits.iter().map(|h| h.entry.ko).collect()
    }

    #[test]
    fn every_entry_and_tree_has_a_japanese_and_a_korean_name() {
        for e in all() {
            assert!(contains_japanese(e.ja), "{e:?}");
            assert!(has_korean(e.ko), "{e:?}");
            for t in e.trees {
                assert!(contains_japanese(t.ja), "{t:?}");
                assert!(has_korean(t.ko), "{t:?}");
                assert!(t.trees.is_empty(), "trees do not nest: {t:?}");
            }
        }
    }

    #[test]
    fn the_nine_classes_each_have_two_trees_and_dungeons_have_none() {
        assert_eq!(CLASSES.len(), 9);
        for class in CLASSES {
            assert_eq!(class.trees.len(), 2, "{class:?}");
        }
        assert!(DUNGEONS.iter().all(|d| d.trees.is_empty()));
    }

    #[test]
    fn the_classes_and_their_trees_are_listed_as_given() {
        let class = |ja: &str| CLASSES.iter().find(|c| c.ja == ja).copied().unwrap();
        let trees = |ja: &str| {
            class(ja)
                .trees
                .iter()
                .map(|t| (t.ja, t.ko))
                .collect::<Vec<_>>()
        };
        assert_eq!(class("ゲイルランサー").ko, "윈드 나이트");
        assert_eq!(class("シールドファイター").ko, "실드 나이트");
        assert_eq!(
            trees("ツインストライカー"),
            [("双炎", "무상"), ("炎舞", "적홍")]
        );
        assert_eq!(
            trees("ストームブレイド"),
            [("雷刃", "발도"), ("月影", "월광")]
        );
        assert_eq!(
            trees("ゲイルランサー"),
            [("烈風", "질풍"), ("乱風", "난무")]
        );
        assert_eq!(
            trees("フロストメイジ"),
            [("氷牙", "얼음창"), ("霜天", "얼음빔")]
        );
        assert_eq!(
            trees("ディバインアーチャー"),
            [("狼弓", "늑대활"), ("鷹弓", "매활")]
        );
        assert_eq!(
            trees("ヘヴィガーディアン"),
            [("剛身", "방패"), ("剛守", "가드")]
        );
        assert_eq!(
            trees("シールドファイター"),
            [("光砕", "방패"), ("光盾", "광휘")]
        );
        assert_eq!(
            trees("ヴァーダントオラクル"),
            [("威咲", "심판"), ("森癒", "치유")]
        );
        assert_eq!(
            trees("ビートパフォーマー"),
            [("狂音", "음파"), ("響奏", "협주")]
        );
    }

    #[test]
    fn no_name_is_listed_twice_in_a_section_or_in_a_class() {
        for section in Section::ALL {
            let entries = section.entries();
            for (i, a) in entries.iter().enumerate() {
                for b in &entries[i + 1..] {
                    assert!(
                        key(a.ja) != key(b.ja) && key(a.ko) != key(b.ko),
                        "{a:?} {b:?}"
                    );
                }
                // "방패" is a tree of two classes -- but never twice in one.
                for (j, x) in a.trees.iter().enumerate() {
                    for y in &a.trees[j + 1..] {
                        assert!(
                            key(x.ja) != key(y.ja) && key(x.ko) != key(y.ko),
                            "{x:?} {y:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_empty_search_lists_everything_with_all_trees() {
        for section in Section::ALL {
            for q in ["", "  "] {
                let hits = search(section, q);
                assert_eq!(hits.len(), section.entries().len());
                for hit in &hits {
                    assert_eq!(hit.trees.len(), hit.entry.trees.len());
                }
            }
        }
    }

    #[test]
    fn a_search_matches_either_language() {
        let by_ja = search(Section::Classes, "ストーム");
        let by_ko = search(Section::Classes, "스톰");
        assert_eq!(by_ja, by_ko);
        assert_eq!(by_ja.len(), 1);
        assert!(by_ja[0].entry.ko.contains("스톰"));
    }

    #[test]
    fn a_class_hit_on_its_own_name_keeps_both_trees() {
        let hits = search(Section::Classes, "스톰");
        assert_eq!(hits[0].trees.len(), 2);
    }

    #[test]
    fn a_hit_on_a_tree_name_shows_its_class_with_only_that_tree() {
        for query in ["雷刃", "발도"] {
            let hits = search(Section::Classes, query);
            assert_eq!(names(&hits), ["스톰 블레이드"], "{query}");
            let trees: Vec<_> = hits[0].trees.iter().map(|t| t.ja).collect();
            assert_eq!(trees, ["雷刃"], "{query}");
        }
    }

    #[test]
    fn a_tree_name_shared_by_two_classes_finds_both() {
        let hits = search(Section::Classes, "방패");
        assert_eq!(names(&hits), ["헤비 가디언", "실드 나이트"]);
        for hit in &hits {
            assert_eq!(hit.trees.len(), 1);
            assert_eq!(hit.trees[0].ko, "방패");
        }
    }

    #[test]
    fn a_tree_with_two_japanese_names_is_found_by_either() {
        for query in ["威咲", "イサキ", "심판"] {
            let hits = search(Section::Classes, query);
            assert_eq!(names(&hits), ["실반 오라클"], "{query}");
            assert_eq!(hits[0].trees.len(), 1, "{query}");
            assert_eq!(hits[0].trees[0].ja, "威咲");
            assert_eq!(hits[0].trees[0].also, "イサキ");
        }
        // Only that tree carries a second name.
        assert!(
            CLASSES
                .iter()
                .flat_map(|c| c.trees)
                .filter(|t| !t.also.is_empty())
                .count()
                == 1
        );
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
