//! Favorites tabs by id: reading a `config.json` written before tabs had ids.
//!
//! Before 0.6.2 a tab *was* its name: `favorite_tabs` was a list of names and a
//! message's `tab` was one of them (`""` = the default tab). Now a tab is
//! `{ "id", "name" }` and a message's `tab` is an id (0 = the default tab).
//! [`migrate_favorites`] rewrites the old shape into the new one on the raw
//! JSON, before it becomes an `AppConfig`: turning a name into an id needs the
//! tab list and the messages together, which no per-field deserializer sees.
//! Pure, and idempotent -- a file already in the new shape is left as it is.

use resonance_types::DEFAULT_FAVORITE_TAB;
use serde_json::{json, Map, Value};

/// Rewrites the favorites of a `config.json` value from tab names to tab ids;
/// returns whether anything changed.
///
/// - each old tab name becomes a tab with the next free id, in the same order;
///   two tabs of one name (hand-edited) stay two tabs;
/// - a message whose `tab` names a tab gets that tab's id (the first, if the
///   name repeats); `""`, a name nobody has, or a blank one goes to the default
///   tab;
/// - nothing is invented: a config without favorites is left alone (the loader
///   gives it the default messages), and a tab or `tab` value of no usable type
///   is dropped.
pub fn migrate_favorites(config: &mut Value) -> bool {
    let Some(root) = config.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    let by_name = migrate_tabs(root, &mut changed);
    migrate_messages(root, &by_name, &mut changed);
    changed
}

/// Converts `favorite_tabs`; returns (name, id) of every converted old tab.
fn migrate_tabs(root: &mut Map<String, Value>, changed: &mut bool) -> Vec<(String, u32)> {
    let Some(Value::Array(tabs)) = root.get_mut("favorite_tabs") else {
        return Vec::new();
    };
    let mut next_id = tabs
        .iter()
        .filter_map(|t| t.get("id").and_then(Value::as_u64))
        .max()
        .map_or(DEFAULT_FAVORITE_TAB + 1, |m| m as u32 + 1)
        .max(DEFAULT_FAVORITE_TAB + 1);
    let mut by_name = Vec::new();
    let mut kept = Vec::with_capacity(tabs.len());
    for tab in tabs.drain(..) {
        match tab {
            Value::String(name) => {
                by_name.push((name.clone(), next_id));
                kept.push(json!({ "id": next_id, "name": name }));
                next_id += 1;
                *changed = true;
            }
            tab @ Value::Object(_) => kept.push(tab),
            _ => *changed = true,
        }
    }
    *tabs = kept;
    by_name
}

fn migrate_messages(root: &mut Map<String, Value>, by_name: &[(String, u32)], changed: &mut bool) {
    let Some(Value::Array(messages)) = root.get_mut("favorite_messages") else {
        return;
    };
    for message in messages.iter_mut().filter_map(Value::as_object_mut) {
        let id = match message.get("tab") {
            None => continue,
            Some(Value::Number(_)) => continue,
            Some(Value::String(name)) => by_name
                .iter()
                .find(|(tab, _)| !name.trim().is_empty() && tab.trim() == name)
                .map_or(DEFAULT_FAVORITE_TAB, |(_, id)| *id),
            Some(_) => DEFAULT_FAVORITE_TAB,
        };
        message.insert("tab".into(), json!(id));
        *changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use resonance_types::{FavoriteMessage, FavoriteTab};

    fn msg(text: &str, tab: Value) -> Value {
        json!({ "text": text, "note": "", "shortcut": "", "tab": tab })
    }

    fn migrated(mut config: Value) -> Value {
        migrate_favorites(&mut config);
        config
    }

    fn tab_ids(config: &Value) -> Vec<(u64, String)> {
        config["favorite_tabs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t["id"].as_u64().unwrap(),
                    t["name"].as_str().unwrap().into(),
                )
            })
            .collect()
    }

    fn message_tabs(config: &Value) -> Vec<u64> {
        config["favorite_messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["tab"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn old_tab_names_become_tabs_with_ids_in_the_same_order() {
        let config = migrated(json!({
            "favorite_tabs": ["레이드", "던전"],
            "favorite_messages": [
                msg("a", json!("")),
                msg("b", json!("던전")),
                msg("c", json!("레이드")),
            ],
        }));
        assert_eq!(
            tab_ids(&config),
            [(1, "레이드".to_string()), (2, "던전".to_string())]
        );
        assert_eq!(message_tabs(&config), [0, 2, 1]);
    }

    #[test]
    fn a_message_in_an_unknown_or_blank_tab_goes_to_the_default_tab() {
        let config = migrated(json!({
            "favorite_tabs": ["레이드"],
            "favorite_messages": [
                msg("a", json!("사라진 탭")),
                msg("b", json!("  ")),
                msg("c", json!(null)),
            ],
        }));
        assert_eq!(message_tabs(&config), [0, 0, 0]);
    }

    #[test]
    fn a_name_used_twice_stays_two_tabs_and_its_messages_go_to_the_first() {
        let config = migrated(json!({
            "favorite_tabs": ["레이드", "레이드"],
            "favorite_messages": [msg("a", json!("레이드"))],
        }));
        assert_eq!(
            tab_ids(&config),
            [(1, "레이드".to_string()), (2, "레이드".to_string())]
        );
        assert_eq!(message_tabs(&config), [1]);
    }

    #[test]
    fn a_hand_edited_name_with_spaces_still_matches_its_messages() {
        // The old loader trimmed tab names but not the messages' tab.
        let config = migrated(json!({
            "favorite_tabs": [" 레이드 "],
            "favorite_messages": [msg("a", json!("레이드"))],
        }));
        assert_eq!(message_tabs(&config), [1]);
    }

    #[test]
    fn messages_saved_without_a_tab_are_left_alone() {
        let mut config = json!({
            "favorite_tabs": ["레이드"],
            "favorite_messages": [{ "text": "a" }],
        });
        assert!(migrate_favorites(&mut config));
        assert!(config["favorite_messages"][0].get("tab").is_none());
        let loaded: FavoriteMessage =
            serde_json::from_value(config["favorite_messages"][0].clone()).unwrap();
        assert_eq!(loaded.tab, DEFAULT_FAVORITE_TAB);
    }

    #[test]
    fn a_config_without_favorites_is_left_alone() {
        let mut config = json!({ "init_done": true });
        assert!(!migrate_favorites(&mut config));
        assert_eq!(config, json!({ "init_done": true }));

        let mut empty = json!({ "favorite_tabs": [], "favorite_messages": [] });
        assert!(!migrate_favorites(&mut empty));
        assert!(!migrate_favorites(&mut json!("not an object")));
    }

    #[test]
    fn the_new_shape_is_read_as_it_is_and_migrating_twice_changes_nothing() {
        let new_shape = json!({
            "favorite_tabs": [{ "id": 4, "name": "레이드" }, { "id": 9, "name": "레이드" }],
            "favorite_messages": [msg("a", json!(9)), msg("b", json!(0))],
        });
        let mut once = new_shape.clone();
        assert!(!migrate_favorites(&mut once));
        assert_eq!(once, new_shape);

        let mut old = json!({
            "favorite_tabs": ["레이드"],
            "favorite_messages": [msg("a", json!("레이드"))],
        });
        assert!(migrate_favorites(&mut old));
        let first = old.clone();
        assert!(!migrate_favorites(&mut old));
        assert_eq!(old, first);
    }

    #[test]
    fn a_tab_of_no_usable_type_is_dropped() {
        let config = migrated(json!({
            "favorite_tabs": ["레이드", null, 7, { "id": 5, "name": "던전" }],
            "favorite_messages": [],
        }));
        assert_eq!(
            tab_ids(&config),
            [(6, "레이드".to_string()), (5, "던전".to_string())]
        );
    }

    #[test]
    fn new_ids_start_after_the_largest_existing_one() {
        let config = migrated(json!({
            "favorite_tabs": [{ "id": 3, "name": "a" }, "b"],
            "favorite_messages": [],
        }));
        assert_eq!(
            tab_ids(&config),
            [(3, "a".to_string()), (4, "b".to_string())]
        );
    }

    /// The shape 0.6.1 wrote, as the app reads it today: every favorite and tab
    /// is still there, in the same tabs and order.
    #[test]
    fn a_real_shaped_0_6_1_config_loads_into_the_new_types() {
        let mut config: Value = serde_json::from_str(
            r#"{
              "init_done": true,
              "favorite_tabs": ["레이드", "던전"],
              "favorite_messages": [
                {"text": "こんにちは！", "note": "안녕하세요", "shortcut": "", "tab": ""},
                {"text": "回復お願いします", "note": "힐 부탁", "shortcut": "Alt+F1", "tab": "레이드"},
                {"text": "もう一回", "note": "한 번 더", "shortcut": "Alt+F2", "tab": "던전"},
                {"text": "ありがとう", "note": "", "shortcut": "", "tab": "레이드"}
              ]
            }"#,
        )
        .unwrap();
        migrate_favorites(&mut config);

        let tabs: Vec<FavoriteTab> =
            serde_json::from_value(config["favorite_tabs"].clone()).unwrap();
        assert_eq!(
            tabs,
            [
                FavoriteTab {
                    id: 1,
                    name: "레이드".into()
                },
                FavoriteTab {
                    id: 2,
                    name: "던전".into()
                },
            ]
        );
        let messages: Vec<FavoriteMessage> =
            serde_json::from_value(config["favorite_messages"].clone()).unwrap();
        let filed: Vec<(&str, u32)> = messages.iter().map(|m| (m.text.as_str(), m.tab)).collect();
        assert_eq!(
            filed,
            [
                ("こんにちは！", 0),
                ("回復お願いします", 1),
                ("もう一回", 2),
                ("ありがとう", 1),
            ]
        );
        assert_eq!(messages[1].shortcut, "Alt+F1", "shortcuts survive");
        assert_eq!(
            config["init_done"], true,
            "the rest of the file is untouched"
        );
    }
}
