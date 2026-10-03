# K26 -- favorites tabs by stable id (planned 2026-10-03, for 0.6.2)

Decided by Kade: **stable ids** (not position keys -- he may want to move tabs later), **no
backup file**, a blank tab name becomes **"탭 N"**, duplicate names allowed, **after 0.6.1**.
Nothing is built yet. Why ids and not positions: with positions every reorder renames every
key; with an id a tab is the same tab wherever it sits in the list, so "move tab" later is just
reordering `tabs`.

## Today
`FavoritesState { messages: Vec<FavoriteMessage>, tabs: Vec<String> }`; `FavoriteMessage.tab` is
a tab *name* (`""` = the default tab, which is not in `tabs`). A tab is its name, so a repeat is
refused (`TabError::Taken`) and `clean_tabs` drops repeats on load. The backend only ever uses
the flat list (text + shortcut). Table rows are keyed by the message's place in the flat list.

## Target
- `FavoriteTab { id: u32, name: String }`; `FavoritesState { messages, tabs: Vec<FavoriteTab> }`;
  `FavoriteMessage.tab: u32` (a tab id). **The default tab is id 0**, implicit, not in `tabs`,
  first and not removable -- exactly as today. A new tab gets `max(ids, 0) + 1`; an id is never
  reused while the tab lives. Vec order = display order.
- The flat message list stays: **`shortcut.rs`, the shortcut re-registering, `find_conflict`, the
  row keys and `save_favorites` keep their shape** -- this is why ids are the cheaper design.
- `TabError::Taken` goes (same name twice is fine, also "기본"); `Empty` and `TooLong` stay. A
  blank or hand-edited name on load becomes "탭 N" (N = its place + 1); an over-long one is cut
  to 12 characters.

## Migration (config.json, on load -- the app owns the file)
Old: `favorite_messages: [{text, note, shortcut, tab: "<name>"}]`, `favorite_tabs: ["<name>"]`.
New: `favorite_tabs: [{id, name}]`, messages carry `tab: <id>`. Decision to make when building:
read the old `tab` string and the new number with one tolerant deserializer (keep the field
name), or read the old fields under their names and write new ones under new names. Either way:
- each old tab name -> a tab with the next id, in the same order; a message whose tab is `""`,
  unknown, or blank goes to id 0 (what `normalize` does today); duplicate old names (hand-edited)
  stay separate tabs, their messages going to the first;
- no `favorites` at all (a config from before favorites) -> the default messages under id 0;
- a file already in the new shape is read as it is; migrating twice changes nothing;
- no backup file (Kade). The rc candidates share the data folder: a tester going back to 0.6.x
  would see defaults and overwrite -- accepted.
Pure function in `crates/types` with tests: old -> new, orphans, duplicate names, an empty or
missing list, idempotence, a real-shaped config as a fixture.

## Touch list
`crates/types/src/lib.rs` (types, migration, tests) - `src-tauri/src/config/app_config.rs` (both
favorites fields, load, `favorites()` / `with_favorites` / `save_favorites`, its tests) -
`src/ui_types.rs` (`AppConfig` mirror, same field names) - `src/config_signals.rs`
(`apply_favorites` cleans like a load; `favorite_tabs` becomes `Vec<FavoriteTab>`; its tests) -
`src/favorites.rs` (pure rules: `add_tab` returns the id, `delete_tab(id)`, `tab_summary(id)`,
`fill_with_defaults(id)`, `normalize` re-files a message whose tab id is gone, `locate`'s label
from the tab's name; `add_from_chat` -> id 0) - `src/components/favorites_window.rs` (`active_tab`
becomes an id, the strip is built from `tabs`, `indices_in_tab(list, id)`) - `chat_row.rs` (no
change beyond the type). `shortcut.rs` is untouched.

## Order and checks
One PR, test first: types + migration (core gate) -> `favorites.rs` rules -> signals / window ->
app config + load (cross-check compile only on Linux; CI's Windows job runs its tests) ->
`ui-preview` of the tab strip (add a duplicate-named tab, delete one, switch). **Kade on
Windows, with his real config:** every favorite and tab is still there, in the same tabs and
order; the shortcuts still paste into the game; a duplicate tab name works; deleting a tab asks
and removes only that tab's messages; the star adds to 기본.

## Not in this task
Renaming a tab and moving a tab (the id design keeps both cheap -- a later PR).
