# Unverified on Windows

The Linux cloud sessions cannot build `src-tauri/` (WinDivert, `windows-sys`, GTK
missing). Anything listed here compiled only in CI, or not at all, and has not been
run as the real app.

- **CI workflow (phase 1).** First run (36549688320): Linux job green; Windows job
  **built and linked** `src-tauri` (so the no-Npcap-SDK assumption held) and failed one
  test — a stale mock, fixed 2026-09-29 (see sessions/). Original note:
  `.github/workflows/ci.yml` had never run. The Windows
  job assumes no Npcap SDK is needed: `pcap` 2.5's build script falls back to version
  1.0.0 when `wpcap.dll` is absent, and nothing links it because no code uses the crate
  (see `unused-deps.md`). If the job fails on linking, that assumption is the first suspect.
  It also relies on a placeholder `dist/index.html` for `generate_context!`.
- **Phase 2 (`crates/core`, `crates/types`).** `cargo check` of the app for
  `x86_64-pc-windows-gnu` passes (19 warnings vs 22 before — the removed ones were
  unused imports of moved code). Not run: app tests (`translator` mock-server test),
  linking, and the app itself. Wire format: the ui's `ChatMessage` now also carries
  `unknownFields` (the app always sent it; the ui used to ignore it).
- **Phase 3 (`lib.rs` split).** Cross-check only. Worth a real run: tray menu (all four
  items), global tab shortcut, Exit → `llama-server.exe` killed. These are the paths
  whose code moved.
- **Phase 4 (`App` / `Settings` split).** wasm check only; the UI has not been opened.
  Worth a click-through: first-run wizard download, settings modal (each section,
  close/reopen keeps typed keyword and last export result), app/model update modals,
  tray toggles reflected in the UI.
- **Phase 5a (version source).** `tauri.conf.json` no longer has `"version"`; Tauri should
  fall back to src-tauri's Cargo version (0.4.0, inherited from the workspace). Check
  the NSIS installer name/version and that the update check still reads 0.4.0
  (`app.package_info().version` in `downloader/gist.rs`). `Cargo.lock` was generated in
  this session from the local registry cache — Kade's previous local lock was untracked,
  so its exact versions may differ.
