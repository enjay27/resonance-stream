# Unverified on Windows

The Linux cloud sessions cannot build `src-tauri/` (WinDivert, `windows-sys`, GTK
missing). Anything listed here compiled only in CI, or not at all, and has not been
run as the real app.

- **CI workflow (phase 1).** First run (36549688320): Linux job green; Windows job
  **built and linked** `src-tauri` (so the no-Npcap-SDK assumption held) and failed one
  test — a stale mock, fixed 2026-09-29 (see sessions/); run 36551390327 on 182d651
  is fully green, so app build + tests are now verified on Windows. Original note:
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
- **Review fixes (2026-09-29, fbfd552..HEAD).** Core logic is tested on Linux (78 + 8 UI
  + 3 types tests); app wiring is cross-checked and CI-built only. Worth a real run:
  chat capture incl. long/bursty world chat (new framing -- confirm with a captured
  fixture), translation on/off/tier change (one llama-server, new port if 8080 busy,
  Exit kills it), dictionary edit applies without restart, model/app/AI-engine
  download (`.part` then rename; engine zip hash pinned), archive now in the app data
  folder, block/unblock, restart sniffer, tab/search/unread badges.
- **feat/ui-improve port (2026-09-29, a5b1966..).** Cross-checked only. Worth a real run:
  firewall-missing → setup wizard; model download skip when the hash matches; restart
  with archiving on reloads chat (no duplicates after re-login); right-click tab menu
  (limits, archive toggle, custom channels); message spacing; window position restored;
  no silent dictionary sync unless auto-sync is on.
- **Translation ledger (2026-09-30, `claude/translation-ledger`).** Core tested on Linux
  (`workers::TranslationLedger`, `load_recent` dedup); app cross-checked (incl. `--tests`).
  Worth a real run: with translation off, collect some Japanese chat, turn it on -- the
  badge shows CATCHING UP, then those rows get translations (newest 100); new chat during
  the catch-up is translated without waiting; a tier change mid-catch-up stops the old
  worker; with archiving on, a restart reloads each caught-up message once.
- **Favorite messages (2026-09-30, `claude/favorite-messages`).** Core/types/ui tests on
  Linux; app cross-checked (incl. `--tests`) only. Worth a real run, as Administrator:
  ⭐ in the nav controls opens the list; 📋 copies; ✏️ edits text + records a shortcut
  (modifier required, F-keys alone OK; conflicts with tab-switch/another favorite refused);
  add / 🗑 (click twice). With the game focused and its chat box open, the shortcut
  should paste the message (clipboard + synthetic Ctrl+V on key release). **Open risk:**
  the game or its anti-cheat may ignore `SendInput` -- if so, text is still on the
  clipboard. Also: changing the tab-switch shortcut must keep favorite shortcuts working
  (all shortcuts now re-registered together in `shortcut.rs::apply_global_shortcuts`).
- **Chat log retention (2026-09-30, `claude/chat-log-retention`).** Core tested on Linux
  (`history::expired_chat_logs`), ui + app cross-checked. Worth a real run: Settings →
  데이터 → "채팅 로그 보관 기간" 2 deletes older `chat_logs/*.jsonl` on save, at start-up
  and at midnight while archiving; 0 deletes nothing; `dataset_raw.jsonl` is untouched.
- **Favorites follow-up (2026-09-30, `claude/favorites-notes-guard`).** Same gates as above.
  Worth a real run: ⭐ on a chat row (hover, both normal and compact view) adds the
  message with its translation as the note, a second ⭐ on the same text adds nothing;
  a quick double press of a favorite shortcut pastes once; after a paste in the game the
  clipboard holds what it held before (500 ms `CLIPBOARD_RESTORE_DELAY` in
  `shortcut.rs` -- if the game pastes the *old* text, raise it). Configs saved by #32
  keep their entries without notes (defaults only apply to a fresh config).
- **N11/N12/N13 (2026-09-30, `claude/review-n11-n12-n13`).** Core + ui tested on Linux,
  app cross-checked (incl. `--tests`). The sniffer restart badge was run on Windows by Kade
  (2026-10-01): works. Still worth a real run: with archiving on, a busy WORLD day no longer pushes GUILD/PARTY
  out of the reloaded history (each channel reloads up to its own tab limit); a blocked
  sender's message raises no unread badge.
- **llama client timeouts (2026-09-30, `claude/integration-test-harness`).** The
  translator worker and `wait_for_server` use `resonance_llama::client()` (2 s connect,
  30 s request); an empty model reply is a failed job. Tested against a mock server on
  Linux; app cross-checked only, not run against a real llama-server.

- **Beginner channel (2026-10-01, `claude/beginner-channel`).** Core/ui/app-cross gates green; not run on Windows: the new 초보자 tab (nav, 🌱 icon, unread badge, colours), its limit and archive entries in the right-click menu, the custom-tab list showing BEGINNER, and the tab-switch shortcut cycle (now six tabs). Beginner lines from old logs stay WORLD.

- **Core-review leftovers (2026-10-01, `claude/core-review-leftovers`).** Core gate green (W4, W6, W7 tested). Not run on Windows: W9 (the sniffer loop backing off on a failing socket read -- nothing tests a failing socket) and W8 (translator retry when llama-server exits while loading: try a bad model file or a taken port and watch for "Retrying in Ns..." then the Error after 3 tries).

- **Settings sidebar + window grow (2026-09-30, `claude/settings-sidebar`).** Core/ui/app-cross gates green; the new layout was screenshotted in Chromium with the wasm build and a mocked `__TAURI__` (grow on open, restore with the returned rect on close). Not run on Windows: `grow_window`/`restore_window` on a real monitor (DPI scaling, work area with the taskbar, a second monitor), the transparent overlay while enlarged. Known gap: closing the app while settings is open leaves the enlarged size for `tauri-plugin-window-state` to save.

- **System-log dedup (`claude/log-spam-dedup`, 2026-10-02).** Core gate green (`log_throttle`, 9 tests); app cross-check only. Worth a real run: leave the game idle in a lobby for 2+ minutes -- the system tab should show one "No game traffic for 15s." and then "(repeated N more times)" about a minute later, not a line every 15 s; start the translator and check the system tab no longer fills with "Polling .../health..." (set log level to trace).
- **Firewall rule per exe (2026-10-02, `candidate/firewall-per-exe`).** Cross-checked only; `netsh` was never run. Check on Windows: (1) `cargo tauri dev` -> wizard -> allow -> chat is captured; (2) run the installed/release exe -> the wizard appears once (its rule does not exist yet) -> allow -> captured; (3) back to dev: no wizard, still captured (`netsh advfirewall firewall show rule name=all | findstr "Packet Sniffing"` lists two rules, each with its own `Program:`); (4) the old un-hashed rule is gone after the first setup.
