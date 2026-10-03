//! The cheat sheet: class and dungeon names in Korean (the main name) with their
//! Japanese names -- official and fan names -- so a Korean player can read (and
//! type) what Japanese players say. Static data plus the search over it -- pure,
//! host-tested.
//!
//! The lists are NOT yet the game's official names: they were assembled from
//! fan sites while the official ones were out of reach, and hold only pairs
//! whose two names plainly name the same thing. Check against the official
//! sites, then add the rest here (and clear [`SOURCE_NOTE`]).

/// Shown under the lists until the data is checked against the official sites.
pub const SOURCE_NOTE: &str = "공식 사이트 확인 전 · 팬 사이트 기준 일부 목록";

/// One thing, named in both languages: the Korean name is the main one, and
/// `ja` lists its Japanese names -- the official one first, then the fan names
/// the players write in chat. A class carries its two specializations
/// ("trees"); a tree or a dungeon has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub ko: &'static str,
    pub ja: &'static [&'static str],
    pub trees: &'static [Entry],
}

impl Entry {
    /// The official Japanese name.
    pub fn official(&self) -> &'static str {
        self.ja[0]
    }

    /// The other Japanese names: what the players call it.
    pub fn fan_names(&self) -> &'static [&'static str] {
        &self.ja[1..]
    }
}

const fn entry(ko: &'static str, ja: &'static [&'static str]) -> Entry {
    Entry { ko, ja, trees: &[] }
}

/// A class and its two trees.
const fn class(ko: &'static str, ja: &'static [&'static str], trees: &'static [Entry]) -> Entry {
    Entry { ko, ja, trees }
}

pub const CLASSES: &[Entry] = &[
    class(
        "트윈 스트라이커",
        &["ツインストライカー"],
        &[entry("무상", &["双炎"]), entry("적홍", &["炎舞"])],
    ),
    class(
        "스톰 블레이드",
        &["ストームブレイド"],
        &[entry("발도", &["雷刃"]), entry("월광", &["月影"])],
    ),
    class(
        "윈드 나이트",
        &["ゲイルランサー"],
        &[entry("질풍", &["烈風"]), entry("난무", &["乱風"])],
    ),
    class(
        "프로스트 메이지",
        &["フロストメイジ"],
        &[entry("얼음창", &["氷牙"]), entry("얼음빔", &["霜天"])],
    ),
    class(
        "디바인 아처",
        &["ディバインアーチャー"],
        &[entry("늑대활", &["狼弓"]), entry("매활", &["鷹弓"])],
    ),
    class(
        "헤비 가디언",
        &["ヘヴィガーディアン"],
        &[entry("방패", &["剛身"]), entry("가드", &["剛守"])],
    ),
    class(
        "실드 나이트",
        &["シールドファイター"],
        &[entry("방패", &["光砕"]), entry("광휘", &["光盾"])],
    ),
    class(
        "실반 오라클",
        &["ヴァーダントオラクル"],
        &[entry("심판", &["威咲", "イサキ"]), entry("치유", &["森癒"])],
    ),
    class(
        "비트 퍼포머",
        &["ビートパフォーマー"],
        &[entry("음파", &["狂音"]), entry("협주", &["響奏"])],
    ),
];

pub const DUNGEONS: &[Entry] = &[
    entry("고블린 소굴", &["ゴブリンの巣窟"]),
    entry("거탑 유적", &["巨塔の遺跡"]),
    entry("티나·정신 세계", &["ティナ・精神領域"]),
    entry("거룡의 발톱", &["巨竜の爪痕"]),
    entry("극한 공간", &["極限空間"]),
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

/// Whether `query` (already a [`key`]) is in the entry's own names: Korean,
/// official Japanese or a fan name.
fn names_match(e: &Entry, query: &str) -> bool {
    key(e.ko).contains(query) || e.ja.iter().any(|n| key(n).contains(query))
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

    /// The Korean names of a search's hits, class level only.
    fn names(hits: &[Hit]) -> Vec<&'static str> {
        hits.iter().map(|h| h.entry.ko).collect()
    }

    #[test]
    fn every_entry_and_tree_has_a_korean_name_and_japanese_names() {
        for e in all() {
            for x in std::iter::once(e).chain(e.trees.iter().copied()) {
                assert!(has_korean(x.ko), "{x:?}");
                assert!(!x.ja.is_empty(), "{x:?}");
                assert!(x.ja.iter().all(|n| contains_japanese(n)), "{x:?}");
            }
            for t in e.trees {
                assert!(t.trees.is_empty(), "trees do not nest: {t:?}");
            }
        }
    }

    #[test]
    fn the_first_japanese_name_is_the_official_one_and_the_rest_are_fan_names() {
        let oracle = CLASSES.iter().find(|c| c.ko == "실반 오라클").unwrap();
        let tree = oracle.trees[0];
        assert_eq!(tree.ko, "심판");
        assert_eq!(tree.official(), "威咲");
        assert_eq!(tree.fan_names(), ["イサキ"]);
        assert_eq!(tree.ja, ["威咲", "イサキ"]);
        // The others have the official name only, for now.
        assert!(oracle.trees[1].fan_names().is_empty());
        assert_eq!(CLASSES[0].official(), "ツインストライカー");
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
        let class = |ja: &str| {
            CLASSES
                .iter()
                .find(|c| c.official() == ja)
                .copied()
                .unwrap()
        };
        let trees = |ja: &str| {
            class(ja)
                .trees
                .iter()
                .map(|t| (t.official(), t.ko))
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
    fn no_name_is_listed_twice_in_a_section_in_a_class_or_in_an_entry() {
        let distinct = |a: &Entry, b: &Entry| {
            key(a.ko) != key(b.ko) && a.ja.iter().all(|x| !b.ja.iter().any(|y| key(x) == key(y)))
        };
        for section in Section::ALL {
            let entries = section.entries();
            for (i, a) in entries.iter().enumerate() {
                for b in &entries[i + 1..] {
                    assert!(distinct(a, b), "{a:?} {b:?}");
                }
                // "방패" is a tree of two classes -- but never twice in one.
                for (j, x) in a.trees.iter().enumerate() {
                    for y in &a.trees[j + 1..] {
                        assert!(distinct(x, y), "{x:?} {y:?}");
                    }
                }
                // One entry never lists the same Japanese name twice.
                for x in std::iter::once(a).chain(a.trees) {
                    for (k, n) in x.ja.iter().enumerate() {
                        assert!(!x.ja[k + 1..].iter().any(|m| key(m) == key(n)), "{x:?}");
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
            let trees: Vec<_> = hits[0].trees.iter().map(|t| t.ko).collect();
            assert_eq!(trees, ["발도"], "{query}");
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
    fn a_tree_is_found_by_its_korean_name_its_official_name_or_a_fan_name() {
        for query in ["심판", "威咲", "イサキ"] {
            let hits = search(Section::Classes, query);
            assert_eq!(names(&hits), ["실반 오라클"], "{query}");
            assert_eq!(hits[0].trees.len(), 1, "{query}");
            assert_eq!(hits[0].trees[0].ja, ["威咲", "イサキ"]);
        }
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
