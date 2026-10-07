---
paths:
  - "src-tauri/**"
---

## Conventions (app crate)

- **Tauri commands are `pub` and reach `generate_handler!` through the crate-root
  `pub use <module>::*`.** A new command module needs both. A private `use` of the same
  name in a module shadows its glob re-export (rustc warns) — the item then silently
  stops being exported; call it by path instead.

- **Favorites change only through `save_favorites`** (`FavoritesState`, then the
  `favorites-changed` event reaches every window); `save_config` keeps the stored ones and
  ignores the ones in its payload. A popup window (`open_popup`) saves only what it owns,
  never the whole config -- its copy of the settings may be older than the main window's.
