# Unverified on Windows

The Linux cloud sessions cannot build `src-tauri/` (WinDivert, `windows-sys`, GTK
missing). Anything listed here compiled only in CI, or not at all, and has not been
run as the real app.

- **CI workflow (phase 1).** `.github/workflows/ci.yml` has never run. The Windows
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
