//! The JSON the Tauri boundary carries, spelled out. Renaming a field, or changing a
//! case or an enum's wire name, changes what the ui reads, what the chat log files hold
//! and what the test bridge publishes: a protocol change, not a refactor (CLAUDE.md).
//! Each test pins one type's exact JSON, both ways; change it only on purpose.

use resonance_types::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

/// `value` serializes to exactly `golden`, and `golden` reads back into an equal value.
fn pinned<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: T, golden: Value) {
    assert_eq!(serde_json::to_value(&value).unwrap(), golden);
    let back: T = serde_json::from_value(golden).unwrap();
    assert_eq!(back, value);
}

#[test]
fn a_chat_message_is_camel_case() {
    let mut unknown_fields = std::collections::HashMap::new();
    unknown_fields.insert("chat_40".to_string(), vec![1u8, 2]);
    pinned(
        ChatMessage {
            pid: 5,
            channel: Channel::Guild,
            nickname: "Mika".into(),
            message: "こんにちは".into(),
            timestamp: 1_700_000_000_123,
            uid: 99,
            class_id: 4,
            level: 60,
            sequence_id: 8,
            is_blocked: true,
            translated: Some("안녕하세요".into()),
            nickname_romaji: Some("Mika".into()),
            unknown_fields,
        },
        json!({
            "pid": 5,
            "channel": "GUILD",
            "nickname": "Mika",
            "message": "こんにちは",
            "timestamp": 1_700_000_000_123u64,
            "uid": 99,
            "classId": 4,
            "level": 60,
            "sequenceId": 8,
            "isBlocked": true,
            "translated": "안녕하세요",
            "nicknameRomaji": "Mika",
            "unknownFields": { "chat_40": [1, 2] }
        }),
    );
}

#[test]
fn a_chat_message_without_a_translation_has_nulls_and_no_unknown_fields() {
    pinned(
        ChatMessage {
            pid: 1,
            message: "hi".into(),
            ..Default::default()
        },
        json!({
            "pid": 1,
            "channel": "WORLD",
            "nickname": "",
            "message": "hi",
            "timestamp": 0,
            "uid": 0,
            "classId": 0,
            "level": 0,
            "sequenceId": 0,
            "isBlocked": false,
            "translated": null,
            "nicknameRomaji": null
        }),
    );
}

#[test]
fn a_system_message_is_camel_case_with_a_lowercase_level() {
    pinned(
        SystemMessage {
            pid: 3,
            timestamp: 42,
            level: SystemLogLevel::Warning,
            source: "Sniffer".into(),
            message: "No game traffic for 15s".into(),
        },
        json!({
            "pid": 3,
            "timestamp": 42,
            "level": "warn",
            "source": "Sniffer",
            "message": "No game traffic for 15s"
        }),
    );
}

#[test]
fn a_translation_result_is_snake_case() {
    let value = TranslationResult {
        pid: 7,
        translated: "안녕".into(),
    };
    assert_eq!(
        serde_json::to_value(&value).unwrap(),
        json!({ "pid": 7, "translated": "안녕" })
    );
}

#[test]
fn the_service_states_carry_their_names_and_order() {
    pinned(
        SnifferStatePayload {
            state: SnifferState::Active,
            message: "Listening on port 5003".into(),
            seq: 4,
        },
        json!({ "state": "Active", "message": "Listening on port 5003", "seq": 4 }),
    );
    pinned(
        TranslatorStatePayload {
            state: TranslatorState::LoadingModel,
            message: "".into(),
            seq: 5,
        },
        json!({ "state": "Loading Model", "message": "", "seq": 5 }),
    );
}

#[test]
fn the_favorites_cross_as_messages_and_tabs() {
    pinned(
        FavoritesState {
            messages: vec![FavoriteMessage {
                text: "よろしく".into(),
                note: "잘 부탁".into(),
                shortcut: "Alt+F1".into(),
                tab: 2,
            }],
            tabs: vec![FavoriteTab {
                id: 2,
                name: "레이드".into(),
            }],
        },
        json!({
            "messages": [{ "text": "よろしく", "note": "잘 부탁", "shortcut": "Alt+F1", "tab": 2 }],
            "tabs": [{ "id": 2, "name": "레이드" }]
        }),
    );
}

#[test]
fn a_window_rect_and_a_progress_payload_keep_their_field_names() {
    pinned(
        WindowRect {
            x: -8,
            y: 0,
            width: 900,
            height: 700,
        },
        json!({ "x": -8, "y": 0, "width": 900, "height": 700 }),
    );
    let progress = ProgressPayload {
        current_file: "model.gguf".into(),
        percent: 50,
        total_percent: 25,
    };
    assert_eq!(
        serde_json::to_value(&progress).unwrap(),
        json!({ "current_file": "model.gguf", "percent": 50, "total_percent": 25 })
    );
}

#[test]
fn the_remote_metadata_keeps_the_gists_snake_case() {
    let gist = GistMetadata {
        app: VersionInfo::default(),
        model: VersionInfo {
            latest_version: "m2".into(),
            download_url: "https://example.com/m.gguf".into(),
            release_notes: "n".into(),
            sha256: "ab".into(),
        },
        dictionary: RemoteDictionary {
            version: "d3".into(),
            updated_at: "2026-10-06".into(),
        },
    };
    let want = json!({
        "app": { "latest_version": "", "download_url": "", "release_notes": "", "sha256": "" },
        "model": {
            "latest_version": "m2",
            "download_url": "https://example.com/m.gguf",
            "release_notes": "n",
            "sha256": "ab"
        },
        "dictionary": { "version": "d3", "updated_at": "2026-10-06" }
    });
    assert_eq!(serde_json::to_value(&gist).unwrap(), want);
    let back: GistMetadata = serde_json::from_value(want).unwrap();
    assert_eq!(back.model.sha256, "ab");
}

#[test]
fn the_fixed_value_settings_keep_their_wire_names() {
    assert_eq!(serde_json::to_value(Tier::VeryHigh).unwrap(), "very high");
    assert_eq!(serde_json::to_value(ComputeMode::Gpu).unwrap(), "gpu");
    assert_eq!(
        serde_json::to_value(TabSwitchModifier::NoModifier).unwrap(),
        "None"
    );
    assert_eq!(
        serde_json::to_value(TranslationView::Study).unwrap(),
        "study"
    );
    assert_eq!(
        serde_json::to_value(PopupKind::CheatSheet).unwrap(),
        "cheatsheet"
    );
    assert_eq!(
        serde_json::to_value(PopupKind::Favorites).unwrap(),
        "favorites"
    );
}
