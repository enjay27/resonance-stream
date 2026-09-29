# `AppConfig` is defined twice

`src-tauri/src/config/app_config.rs` and `src/ui_types.rs` each define `AppConfig`.
Phase 2 moved every other duplicated DTO into `crates/types`; this one was left
because unifying it changes behaviour on both sides:

- **ui:** `#[derive(Default)]` (empty strings, 0, false) is the value before the
  backend's config arrives. The app's hand-written `Default` (chat_limit 1000,
  tab "전체", filters…) would replace it.
- **app:** `network_interface` and `drag_to_scroll` have no `#[serde(default)]`
  on the app side, but do on the ui side. Taking the ui's attributes would let an
  old `config.json` missing those keys load instead of failing to parse — a change
  in how `load_config` treats old files.

Either is probably an improvement, but it is a behaviour change, not a move.
Decide, then do it as its own commit.
