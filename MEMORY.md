# Active State — resonance-stream

**Index, not the record.** Only what would be *false* the moment it goes stale lives here.

## Now — 2026-10-01

**Raw capture tool + W2 framing (2026-09-30, `claude/festive-hypatia-5r78o7`):** settings > debug mode > "Raw Capture" (`raw_capture`, live) appends each port-5003 packet as `<unix_ms>\t<hex IPv4 packet>` to `<app data>/captures/capture-*.log` (50 MB cap; `capture/recorder.rs`). Kade's first capture gave the real frame layout (`[u32 len][u16 type][body]`, bit 15 = zstd; table in `protocol/framing.rs` and [`core-review-2026-09-30.md`](.memory/roadmap/core-review-2026-09-30.md)). **W2 done:** length-based `FrameAssembler`, TCP-seq retransmit/gap handling in `StreamTracker`, `0x8003` channel history decompressed (`ruzstd`) and shown oldest-first (`parser::history_pipeline`); the mock's rich-line loss is gone. Real capture pinned (256 packets -> 160 chats) in `crates/core/tests/capture_replay.rs`; the file itself is gitignored (`tests/fixtures/*.capture.log`), CI skips it. Kade ran the tool and W2 on Windows: works. A second capture (channel test lines) is decoded in the roadmap file -- it shows plain `0x0003` history frames W2 still skips, and the channel codes (9 = beginner). **In progress: C2/C3 typed UI, approved -- plan and status in [`.memory/roadmap/c2-c3-typed-ui-plan.md`](.memory/roadmap/c2-c3-typed-ui-plan.md); steps C2a (`Channel`) done, C2b (service states, PR open), C2c, C2d, C3, one merged PR at a time.**

**Per-channel archive (2026-09-30, `claude/per-channel-archive-compact-copy`):** the global `archive_chat` toggle is gone; the archive worker always runs and each tab (right-click, `archive_ignored_channels`) decides. `dataset_raw.jsonl` -> `dataset_<CHANNEL>.jsonl` (`dataset_file_name`, core); daily `chat_logs/` unchanged; write failures reported once as a system message. Old `dataset_raw.jsonl` is left as is. Compact-mode star/COPY on one line. Compact "hide original" fixed: the original bubble carried `inline` and `hidden` together and `inline` won (`compact_original_class`, tested). Kade ran it on Windows (2026-10-01): per-tab files work. Not yet looked at: compact-mode hover.

**Integration tests (2026-09-30, `claude/integration-test-harness`):** mock game server +
mock llama-server in `crates/llama/tests/`; HTTP timeouts fixed. Found framing loses
lines around rich messages -- fixed by W2 (above) --
[`sessions/2026-09-30-integration-tests.md`](.memory/sessions/2026-09-30-integration-tests.md).

**Review round 2 (2026-09-30):** core + UI communication + LLM server —
[`.memory/roadmap/review-2026-09-30-round2.md`](.memory/roadmap/review-2026-09-30-round2.md).
Steps 1-7 in order, one PR each. Steps 1 (A1 `get_service_states`) 2 (A2/A9 translator supervisor), 3 (A3/A5 hydration), 4 (A6 row signal leak), 5 (C1 prompt in core, A8 output limit), 6 (B1 cache), 7a (B6 Japanese rule, B3 server log) done; C2/C3 wait on Kade. **A4 (double `<bos>`) waits on Kade** — question in the file.

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

**Graft 0.21.1 (2026-09-30):** pinned in `graft-bootstrap.cjs`. `.mcp.json` runs `graft mcp` as
graft's wiring refresh writes it (committed, `claude/graft-mcp-wiring`); a session only runs
`graft build`. A later `graft init`/`upgrade` rewrite goes in its own commit.

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
