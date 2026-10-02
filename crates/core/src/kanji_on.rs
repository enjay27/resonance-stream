//! One on'yomi per kanji, in hiragana: the fallback for a word the
//! morphological dictionary does not know (巨塔), which is then read kanji by
//! kanji (きょ + とう). The table is generated from KANJIDIC2 by
//! `scripts/gen_kanji_on.py` -- see `kanji_on_table.rs` for the source and
//! licence.

/// The on'yomi of `c` in hiragana, or `None` for a character the table does
/// not have (kana, Latin, a kanji with no on'yomi such as 畑 or 峠).
pub fn on_reading(c: char) -> Option<&'static str> {
    TABLE
        .binary_search_by_key(&c, |&(kanji, _)| kanji)
        .ok()
        .map(|i| TABLE[i].1)
}

include!("kanji_on_table.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kanji_has_its_on_yomi_in_hiragana() {
        assert_eq!(on_reading('巨'), Some("きょ"));
        assert_eq!(on_reading('塔'), Some("とう"));
        assert_eq!(on_reading('山'), Some("さん"));
    }

    #[test]
    fn the_table_is_sorted_and_every_reading_is_hiragana() {
        assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(TABLE.iter().all(|(_, r)| !r.is_empty()
            && r.chars()
                .all(|c| matches!(c, '\u{3041}'..='\u{3096}' | 'ー'))));
    }

    #[test]
    fn kana_latin_and_kanji_without_on_yomi_have_none() {
        for c in ['あ', 'ア', 'M', '6', '々', '畑', '峠'] {
            assert_eq!(on_reading(c), None, "{c}");
        }
    }
}
