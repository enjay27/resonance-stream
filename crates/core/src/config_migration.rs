//! The settings file carries a `config_version`; this reads it and brings an older file up to the
//! current shape before it becomes an `AppConfig`. The version lives in the JSON only -- it is not
//! a field of `AppConfig`, so the ui never sees or sends it, and the app stamps it on every write.
//!
//! A file without a version is version 0: it runs the whole chain (today the favorites tab ids,
//! `favorites_migration`). A file with a newer version than this app knows is left alone and
//! reported, so a downgrade can keep a copy before a save drops what the newer app added.

use crate::favorites_migration::migrate_favorites;
use resonance_types::AppConfig;
use serde_json::Value;

/// The shape `config.json` has today. Raise it with a step in [`migrate_config`] when a change
/// to the file needs one.
pub const CONFIG_VERSION: u32 = 1;

/// The key the version is stored under.
pub const VERSION_KEY: &str = "config_version";

/// What [`migrate_config`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Migrated {
    /// Already the current version (or not a settings object at all): nothing changed.
    Current,
    /// An older file (no version counts as 0) was brought up to date and stamped.
    Upgraded { from: u32 },
    /// Written by a newer app: left as it is. Saving here drops what that version added.
    Newer { found: u32 },
}

/// The version a settings value carries; 0 when it has none or none that makes sense. A number
/// too big for a version is newer than any this app knows.
pub fn read_version(config: &Value) -> u32 {
    config
        .get(VERSION_KEY)
        .and_then(Value::as_u64)
        .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

/// Brings a settings value up to [`CONFIG_VERSION`], one step per version it is behind, then
/// stamps it. A newer file is left untouched.
pub fn migrate_config(config: &mut Value) -> Migrated {
    if !config.is_object() {
        return Migrated::Current;
    }
    let found = read_version(config);
    if found > CONFIG_VERSION {
        return Migrated::Newer { found };
    }
    if found == CONFIG_VERSION {
        return Migrated::Current;
    }
    if found < 1 {
        // 0 -> 1: favorites tabs by id (before, a tab was its name).
        migrate_favorites(config);
    }
    stamp(config);
    Migrated::Upgraded { from: found }
}

/// Marks a settings value as written by this version.
pub fn stamp(config: &mut Value) {
    if let Some(object) = config.as_object_mut() {
        object.insert(VERSION_KEY.to_string(), Value::from(CONFIG_VERSION));
    }
}

/// The text of `config.json` for `config`: pretty JSON, stamped with the current version.
pub fn to_file_text(config: &AppConfig) -> serde_json::Result<String> {
    let mut value = serde_json::to_value(config)?;
    stamp(&mut value);
    serde_json::to_string_pretty(&value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A file from before favorites tabs had ids: `favorite_tabs` are names.
    fn old_shape() -> serde_json::Value {
        json!({"init_done": true, "favorite_tabs": ["레이드"], "favorite_messages": [{"text": "a", "tab": "레이드"}]})
    }

    #[test]
    fn a_file_without_a_version_runs_the_whole_chain_and_is_stamped() {
        let mut config = old_shape();
        assert_eq!(migrate_config(&mut config), Migrated::Upgraded { from: 0 });
        assert_eq!(config["favorite_tabs"][0]["name"], "레이드");
        assert_eq!(config["favorite_messages"][0]["tab"], 1);
        assert_eq!(config[VERSION_KEY], CONFIG_VERSION);
    }

    #[test]
    fn a_stamped_file_is_current_and_the_chain_does_not_run_again() {
        // Driven by the version, not by looking at the shape: a stamped file in the old shape
        // is left as it is.
        let mut config = old_shape();
        config[VERSION_KEY] = json!(CONFIG_VERSION);
        let before = config.clone();
        assert_eq!(migrate_config(&mut config), Migrated::Current);
        assert_eq!(config, before);
    }

    #[test]
    fn migrating_twice_changes_nothing_the_second_time() {
        let mut config = old_shape();
        migrate_config(&mut config);
        let once = config.clone();
        assert_eq!(migrate_config(&mut config), Migrated::Current);
        assert_eq!(config, once);
    }

    #[test]
    fn a_newer_file_is_reported_and_left_untouched() {
        let mut config = old_shape();
        config[VERSION_KEY] = json!(CONFIG_VERSION + 41);
        let before = config.clone();
        assert_eq!(
            migrate_config(&mut config),
            Migrated::Newer {
                found: CONFIG_VERSION + 41
            }
        );
        assert_eq!(config, before, "not migrated, not re-stamped");
    }

    #[test]
    fn a_version_that_is_no_number_counts_as_none() {
        for odd in [json!("2"), json!(-1), json!(1.5), json!(null), json!([1])] {
            let mut config = old_shape();
            config[VERSION_KEY] = odd.clone();
            assert_eq!(read_version(&config), 0, "{odd}");
            assert_eq!(
                migrate_config(&mut config),
                Migrated::Upgraded { from: 0 },
                "{odd}"
            );
            assert_eq!(config[VERSION_KEY], CONFIG_VERSION);
        }
    }

    #[test]
    fn something_that_is_not_a_config_is_left_alone() {
        let mut not_an_object = json!("not an object");
        assert_eq!(migrate_config(&mut not_an_object), Migrated::Current);
        assert_eq!(not_an_object, json!("not an object"));
    }

    #[test]
    fn writing_stamps_the_current_version_over_an_older_one() {
        let mut config = json!({"init_done": true, "config_version": 0});
        stamp(&mut config);
        assert_eq!(config[VERSION_KEY], CONFIG_VERSION);
    }

    #[test]
    fn the_file_text_of_a_config_carries_the_version_and_reads_back() {
        let config = AppConfig::default();
        let text = to_file_text(&config).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[VERSION_KEY], CONFIG_VERSION);
        // The type does not know the key: it reads the file all the same.
        let back: AppConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(&config).unwrap()
        );
    }
}
