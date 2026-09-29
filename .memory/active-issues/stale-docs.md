# Docs that contradict the code

Found 2026-09-29 while planning the refactor. `README.md` (Korean) is current and the
reference; the rest was fixed or flagged in phase 5b.

- **Fixed (5b):** `BUILD*.md` rewritten (no Python sidecar / `setup_libs.bat` / Npcap;
  trunk + tauri-cli + npm; `just check`). `README_EN.md` re-translated from `README.md`.
- **Deleted (Kade, 2026-09-29):** `TROUBLE_SHOOTING*.md` described the Python sidecar
  and WinDivert. Kade will write new ones; until then README links issues #12 / #13.
- **Settings footer says "Resonance Stream v2.0"** (`src/components/settings/mod.rs`)
  while the app is 0.4.0 and the title bar (now `CARGO_PKG_VERSION`) says v0.4.0.
  Left as is: changing visible text is Kade's call.
- **index.html `<title>` is "Tauri + Leptos App"** (template leftover). Left as is: not
  verified whether Tauri propagates the document title to the window/taskbar.
