# Docs that contradict the code

Found 2026-09-29 while planning the refactor. `README.md` (Korean) is current and the
reference; the rest was fixed or flagged in phase 5b.

- **Fixed (5b):** `BUILD*.md` rewritten (no Python sidecar / `setup_libs.bat` / Npcap;
  trunk + tauri-cli + npm; `just check`). `README_EN.md` re-translated from `README.md`.
- **Deleted (Kade, 2026-09-29):** `TROUBLE_SHOOTING*.md` described the Python sidecar
  and WinDivert. Kade will write new ones; until then README links issues #12 / #13.
- **Fixed (Kade, 2026-09-29):** the Settings footer said "v2.0"; it now shows the app
  version (`CARGO_PKG_VERSION`), like the title bar.
- **index.html `<title>` is "Tauri + Leptos App"** (template leftover). Left as is: not
  verified whether Tauri propagates the document title to the window/taskbar.
