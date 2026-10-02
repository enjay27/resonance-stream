# Active State — resonance-stream

**Index, not the record.** Only what would be *false* the moment it goes stale lives here.

## Now — 2026-10-02

**System-log spam (2026-10-02, `claude/log-spam-dedup`, off `main`):** `resonance_core::log_throttle::LogThrottle` (tested) holds identical `(source, message)` lines back for 60 s and reports "(repeated N more times)" on the next; `inject_system_message_throttled` (`events.rs`) uses it, opt-in -- only the sniffer watchdog ("No game traffic for 15s", every 15 s while idle) so far, because a user action can repeat other lines and those must always show. The per-second `Polling .../health...` line now goes to the log file only (it filled the 200-line system history during a model load). **Open:** the "dictionary" half of the review note -- no repeating dictionary line was found, only two Success lines per sync. NOT VERIFIED on Windows: the app glue (cross-check only).

**Release-candidate dispatch picks a merge (2026-10-01, `claude/rc-build-ui-fixes-ny8j3w`):** a manual run of `release-candidate.yml` has a `which` dropdown -- latest merge into rc (default; also what a push and an empty value mean) or the 2nd..5th newest. GitHub inputs are fixed options, so it counts merges instead of listing PRs; the run summary names the PR taken. A new `pick` job resolves the commit (`rc_rank`, `rc_nth_merge` in `rc-lib.sh`, tested); build and release use it, and only rank 1 re-runs the gates. **rc must get this file** (merge `main` into `rc`): a manual run reads the workflow from the branch it runs on. NOT VERIFIED: a real run (dry-run of the pick against rc's history only); actionlint and shellcheck clean.

**Auto-merge only into `main` (2026-10-01, `claude/rc-build-ui-fixes-ny8j3w`):** `auto-merge.yml` merged every green `claude/*` PR whatever its base, so PRs #76-#78 went into `rc` by themselves -- contrary to CLAUDE.md -- and, because a merge made with `GITHUB_TOKEN` starts no `push` run, `release-candidate.yml` built nothing for them (latest candidate: `v0.6.0-rc.main-ui-a-cb`, without #76-#78). Fix: `gh pr list --base main`, so a PR into `rc` stays open for a person to merge (that merge starts the build). Takes effect once it is on `main`. NOT VERIFIED: a real run (a `workflow_run` workflow cannot run here); actionlint clean. Kade builds the current `rc` himself (workflow_dispatch on `release-candidate.yml`).

**Release candidates (2026-10-01, `claude/rc-release-workflow`):** a merge into `rc` builds a plain exe and publishes prerelease `v0.6.0-rc.<branch>[.n]` (newest 5 kept) -- flow in CLAUDE.md *Release candidates*; logic in `.github/scripts/rc-lib.sh` (tested). Workspace version bumped 0.4.0 -> 0.6.0 (v0.5.0 was already released). First runs (2026-10-01): creating `rc` from main ran it once (`v0.6.0-rc.build`, a plain main build -- Kade deletes it by hand), then PR #74 (`candidate/main-ui-a-cb`). An empty "changes vs main" section now reads "(없음)" (`rc_changes`, `claude/rc-notes-empty`).

**UI check method (2026-10-01, `claude/ui-preview-skill`):** every UI task is screenshotted with the `ui-preview` skill -- wasm build + mocked `__TAURI__` + Playwright, on Linux. Steps and limits in [`.claude/skills/ui-preview/SKILL.md`](.claude/skills/ui-preview/SKILL.md).

**Settings view (2026-09-30, `claude/settings-sidebar`):** macOS-style -- category sidebar (`settings_nav::SettingsCategory`, remembered in `signals.ui.settings_category`) and one pane per category; a window smaller than 900x640 grows while settings is open (`grow_window`/`restore_window`, fit logic `resonance_core::window::grow_to_fit`). Merged (#70). NOT VERIFIED on Windows -- [`unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md).

**Open-items series (2026-10-01): done** -- plain `0x0003` history (#67), `Channel::Beginner` (#68), then `claude/core-review-leftovers`: W4 (dedup grows with reloaded history), W6 (chunk payload before type, one channel-code table, dead `SenderInfo.is_blocked` removed), W7 (IPv6 no longer feeds the watchdog), W9 (`read_error_backoff`), W8 (server that dies while loading is retried with a fresh port via the supervisor). **Still open:** `class_id` (sender tag 24 -- meaning unknown, needs a capture), A4 (Kade's re-fine-tune), compact-mode hover. NOT VERIFIED on Windows: the beginner tab; W8/W9 glue. Handoff: [`sessions/2026-10-01-open-items-handoff.md`](.memory/sessions/2026-10-01-open-items-handoff.md).

**CI build cache (2026-10-01, `claude/friendly-johnson-sqmhjl`):** the Windows gate took ~3m54s; logs showed a ~1 GB rust-cache re-saved by every PR (PR caches are readable by that PR only, and evict main's), `cargo check` + `cargo test` building every dependency twice, and full debug info. Now: only `main` saves the cache (`save-if`), `auto-merge.yml` dispatches CI on main after each merge (GITHUB_TOKEN merges start no push run), `app-check` is `cargo test` only, `CARGO_PROFILE_DEV_DEBUG=line-tables-only`. **Open:** `auto-merge.yml` is read from main, so the first dispatch needs a manual *Run workflow* on main after this merges; compare job times on the next PRs. NOT VERIFIED: the Windows job and `ui-check` (no Windows / wasm target in that session) -- CI decides.

**Raw capture tool + W2 framing (2026-09-30, `claude/festive-hypatia-5r78o7`):** settings > debug mode > "Raw Capture" (`raw_capture`, live) appends each port-5003 packet as `<unix_ms>\t<hex IPv4 packet>` to `<app data>/captures/capture-*.log` (50 MB cap; `capture/recorder.rs`). Kade's first capture gave the real frame layout (`[u32 len][u16 type][body]`, bit 15 = zstd; table in `protocol/framing.rs` and [`core-review-2026-09-30.md`](.memory/roadmap/core-review-2026-09-30.md)). **W2 done:** length-based `FrameAssembler`, TCP-seq retransmit/gap handling in `StreamTracker`, `0x8003` channel history decompressed (`ruzstd`) and shown oldest-first (`parser::history_pipeline`); the mock's rich-line loss is gone. Real capture pinned (256 packets -> 160 chats) in `crates/core/tests/capture_replay.rs`; the file itself is gitignored (`tests/fixtures/*.capture.log`), CI skips it. Kade ran the tool and W2 on Windows: works. A second capture (channel test lines) is decoded in the roadmap file -- it shows plain `0x0003` history frames W2 still skips, and the channel codes (9 = beginner).

**Typed UI refactor C2/C3 (2026-10-01, PRs #56-#65): done** -- `Channel`, `SnifferState`/`TranslatorState`, `Tab`, the settings enums (`ComputeMode`, `Tier`, `Theme`, `LogLevel`, `TabSwitchModifier`) and `SystemLogLevel` live in `resonance-types` (same strings on disk and on the wire, unknown -> default); `AppSignals` is six groups (`config`, `service`, `setup`, `updates`, `chat`, `ui`), the settings one with `to_config()`/`apply()`. Plan and what it found: [`.memory/roadmap/c2-c3-typed-ui-plan.md`](.memory/roadmap/c2-c3-typed-ui-plan.md). **Next candidates:** plain `0x0003` `{3: channel, 5: chat}` history frames (a line is dropped), `Channel::Beginner` (code 9), W4/W6-W9 (see the core review file). NOT VERIFIED on Windows: the C2/C3 surfaces (tabs, badges, settings save/restore, system tab, shortcut).

**Per-channel archive (2026-09-30, `claude/per-channel-archive-compact-copy`):** the global `archive_chat` toggle is gone; the archive worker always runs and each tab (right-click, `archive_ignored_channels`) decides. `dataset_raw.jsonl` -> `dataset_<CHANNEL>.jsonl` (`dataset_file_name`, core); daily `chat_logs/` unchanged; write failures reported once as a system message. Old `dataset_raw.jsonl` is left as is. Compact-mode star/COPY on one line. Compact "hide original" fixed: the original bubble carried `inline` and `hidden` together and `inline` won (`compact_original_class`, tested). Kade ran it on Windows (2026-10-01): per-tab files work. Not yet looked at: compact-mode hover.

**Integration tests (2026-09-30, `claude/integration-test-harness`):** mock game server +
mock llama-server in `crates/llama/tests/`; HTTP timeouts fixed. Found framing loses
lines around rich messages -- fixed by W2 (above) --
[`sessions/2026-09-30-integration-tests.md`](.memory/sessions/2026-09-30-integration-tests.md).

**Review round 2 (2026-09-30):** core + UI communication + LLM server —
[`.memory/roadmap/review-2026-09-30-round2.md`](.memory/roadmap/review-2026-09-30-round2.md).
Steps 1-7 in order, one PR each. Steps 1 (A1 `get_service_states`) 2 (A2/A9 translator supervisor), 3 (A3/A5 hydration), 4 (A6 row signal leak), 5 (C1 prompt in core, A8 output limit), 6 (B1 cache), 7a (B6 Japanese rule, B3 server log) done; C2/C3 done (above). **A4 (double `<bos>`) waits on Kade's re-fine-tune (2026-10-01)** — question in the file.

**Core review follow-up (2026-09-30):** record and status in
[`.memory/roadmap/core-review-2026-09-30.md`](.memory/roadmap/core-review-2026-09-30.md).
Done: W3, P1, W1, W5 (PRs #38-#41) and the field iterator `decoder::Fields` (parser output
pinned by a golden test). W2 done (see above). TDD is the rule (CLAUDE.md); `just coverage`.

**Graft-guided refactor, phases 0–5: done** —
[`.memory/roadmap/refactor-2026-09.md`](.memory/roadmap/refactor-2026-09.md).
Pushed at Kade's request. **CI green on 182d651** (run 36551390327): Linux + Windows,
incl. the app tests — after fixing one stale test (mock served the wrong llama endpoint).

**The app part (`src-tauri/`) only cross-*checks* on Linux** (`just app-cross-check`,
mingw) — it cannot link, so its tests and any run need Windows (CI or Kade's machine). Anything that
skipped it is listed in
[`.memory/active-issues/unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md).

**Docs:** BUILD/README_EN fixed; TROUBLE_SHOOTING deleted (Kade adds new later) —
[`.memory/active-issues/stale-docs.md`](.memory/active-issues/stale-docs.md).

**Code review 2026-09-29:** [`docs/code-review-2026-09-29.md`](docs/code-review-2026-09-29.md) —
every item fixed test-first in 7 commits (fbfd552 onward, see the report's *Status*). Not
done by design: gist host allow-list, reqwest 0.12. B2 framing still wants a real capture.
Windows run needed — [`unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md).

**feat/ui-improve ported onto main** (8 commits, Kade as author; 9937ef2 redundant) —
[`.memory/sessions/2026-09-29-ui-improve-port.md`](.memory/sessions/2026-09-29-ui-improve-port.md).
Kade's calls: sequential pids, both archives, gist has sha256, auto-sync off by default.

**Graft 0.21.1 (2026-10-01, `claude/graft-mcp-wait`):** pinned in `graft-bootstrap.cjs`. `.mcp.json` runs
`node .claude/helpers/graft-mcp.cjs` again: in a cloud session graft is installed by the bootstrap hook
*after* MCP servers spawn, and a bare `graft mcp` failed (ENOENT, 2026-10-01). The helper waits up to 25 s.
graft's refresh does not rewrite it in cloud (bootstrap stamps the current version first); after a local
`graft init`/`upgrade`, re-point it in its own commit.

**Git workflow (2026-09-30):** each task on a `claude/<name>` branch, push when the local gate
is green, PR opened by Claude, `.github/workflows/auto-merge.yml` merges it when CI passes
(needs to be on `main` first). CI runs on every PR (docs-only too); `push` only on `main`.

**Review 2026-09-30:** 13 findings, all fixed (N11-N13 on `claude/review-n11-n12-n13`) —
[`.memory/active-issues/review-2026-09-30.md`](.memory/active-issues/review-2026-09-30.md).
App parts cross-checked only.

**Translation ledger (2026-09-30, `claude/translation-ledger`):** owed Japanese messages of
this run are caught up (newest 100, setting) at each translator start --
[`sessions/2026-09-30-translation-ledger.md`](.memory/sessions/2026-09-30-translation-ledger.md).

**Favorite messages (2026-09-30, `claude/favorite-messages`):** ⭐ list -- copy, edit, global
shortcut pastes into the game (clipboard + `SendInput` Ctrl+V), merged (PR #32). Follow-up
`claude/favorites-notes-guard`: Korean note, ⭐ from chat, 1.5 s repeat guard, old clipboard
restored after 500 ms. Both need a Windows run --
[`unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md).

**Chat log retention (2026-09-30, `claude/chat-log-retention`):** `chat_log_retention_days`
(both AppConfigs, default 0 = keep all), pruned at start-up / on save / at day change.

## Where the detail is

| read | when |
|---|---|
| [`CLAUDE.md`](CLAUDE.md) | rules, gates, layout |
| [`.memory/README.md`](.memory/README.md) | which memory file takes what |
| [`.memory/active-issues/`](.memory/active-issues/) | before trusting a doc, a build, or a behaviour |
| [`.memory/roadmap/`](.memory/roadmap/) | what is next, and what proves it done |
| [`.memory/sessions/`](.memory/sessions/) | why a decision was made, including wrong turns |

## Rules
- **Under ~40 lines.** *Now* carries what is next and what is unverified; nothing else.
- **Update *Now* every session**, even when the answer is "unchanged".
