//! The custom dictionary this repo publishes (`metadata/custom_dict.json`): what it holds, and the two
//! rules that keep it safe to ship to every user.
//!
//! The app flattens every category into one map keyed by the Japanese text and shields each term it
//! finds in a chat line, so a key must mean one thing, and a one-character key hits ordinary words
//! (`始` in `始める`, `終` in `終わり`).

use resonance_core::text::Dictionary;
use std::collections::{BTreeMap, BTreeSet};

const SOURCE: &str = include_str!("../../../metadata/custom_dict.json");

/// `{ category: { ja: ko } }`, as the file has it.
fn categories() -> BTreeMap<String, BTreeMap<String, String>> {
    serde_json::from_str(SOURCE).expect("the dictionary is categorised JSON")
}

/// Every Japanese key with each Korean it has, across all categories.
fn meanings() -> BTreeMap<String, BTreeSet<String>> {
    let mut all: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for terms in categories().values() {
        for (ja, ko) in terms {
            all.entry(ja.clone()).or_default().insert(ko.clone());
        }
    }
    all
}

#[test]
fn the_dictionary_is_one_the_app_can_read() {
    let dictionary = Dictionary::from_json_str(SOURCE).expect("it parses");
    assert!(dictionary.len() > 100, "{} terms", dictionary.len());
}

#[test]
fn a_key_that_is_in_two_categories_has_the_same_korean_in_both() {
    // One map is built from all categories: two meanings would leave the winner to the map's order.
    let clashes: Vec<_> = meanings()
        .into_iter()
        .filter(|(_, ko)| ko.len() > 1)
        .collect();
    assert!(clashes.is_empty(), "keys with two meanings: {clashes:?}");
}

#[test]
fn the_only_one_character_terms_are_the_known_ones() {
    // A one-character term is shielded wherever the character stands, inside words too. These three
    // are deliberate (`凸` is the game's word for a breakthrough, `〆` closes a recruiting call, `＠`
    // is kept as it is); a new one needs a reason, so it is listed here on purpose.
    let one_character: BTreeSet<_> = meanings()
        .into_keys()
        .filter(|ja| ja.chars().count() == 1)
        .collect();
    let known: BTreeSet<_> = ["〆", "凸", "＠"].map(String::from).into();
    assert_eq!(one_character, known);
}

#[test]
fn season_3_dungeons_are_named_as_the_cheat_sheet_names_them() {
    // The official Japanese name and the short name the players write, for each season 3 dungeon.
    // Left out on purpose: the one-character stage names (墓, 始, 継, 終), `ティナ` and `巨塔`, which
    // already mean the NPC and the tower (`티나`, `거탑`), and `極限空間`, spelt `극한 공간`.
    let want = [
        ("呪われし煌墓", "저주받은 무덤"),
        ("機械化工場", "기계화 처리소"),
        ("工場", "기계화 처리소"),
        ("蝕・ティナの精神領域", "침식 티나"),
        ("霧海の猟場", "안개 속 사냥터"),
        ("霧海", "안개 속 사냥터"),
        ("蝕・巨塔の遺跡", "침식 거탑"),
        ("珊瑚岩の谷", "환해 암초"),
        ("珊瑚", "환해 암초"),
        ("幻夢レイド", "환상꿈 레이드"),
        ("迷妄の森", "미망의 숲"),
        ("迷妄", "미망의 숲"),
    ];
    let all = categories();
    let dungeon = &all["dungeon"];
    for (ja, ko) in want {
        assert_eq!(dungeon.get(ja).map(String::as_str), Some(ko), "{ja}");
    }
    // What was already there is untouched.
    assert_eq!(dungeon["極限空間"], "극한 공간");
    assert_eq!(dungeon["巨塔"], "거탑");
    assert_eq!(all["npc"]["ティナ"], "티나");
}

#[test]
fn the_classes_and_their_trees_are_named_as_the_cheat_sheet_names_them() {
    let want = [
        ("ツインストライカー", "트윈 스트라이커"),
        ("双炎型", "무상"),
        ("炎舞型", "적홍"),
        ("雷刃", "발도"),
        ("烈風", "질풍"),
        ("乱風", "난무"),
        ("剛守", "가드"),
        ("狂音", "음파"),
        ("響奏", "협주"),
        // 실드 나이트: the cheat sheet is right, the dictionary had the two trees the other way round
        // (光砕型 was "광휘의 축복", 光盾型 "신성한 방패").
        ("光砕型", "방패"),
        ("光盾型", "광휘"),
        ("光砕", "방패"),
        ("光盾", "광휘"),
    ];
    let all = categories();
    let classes = &all["class"];
    for (ja, ko) in want {
        assert_eq!(classes.get(ja).map(String::as_str), Some(ko), "{ja}");
    }
}

#[test]
fn a_monster_is_named_as_what_it_is() {
    // Was "리자드맨 마법사사": a doubled syllable.
    assert_eq!(categories()["monster"]["ギルミーメイジ"], "리자드맨 마법사");
}
