# Active State — resonance-stream

**Index, not the record.** Only what would be *false* the moment it goes stale lives here.

## Now — 2026-09-30

**Core review follow-up (`claude/tdd-rule-and-core-hardening`):** CLAUDE.md now requires TDD
(test first, `just coverage`). Core coverage 96.7% lines. W3 done (PR #38). P1 done: `load_recent` examines at most
`MAX_SCAN_LINES` = 50k lines (start-up 1.6 s -> 54 ms on 60 days of logs; quiet channels reload
fewer messages). W1 done: stream key is the full
4-tuple (`StreamKey`, 12 bytes), idle connections dropped after 60 s. Next, in order: P2+P3 `Arc<Dictionary>` + matcher, W5 placeholder check, field iterator, W2.

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

**Graft 0.21.1 (2026-09-30, `claude/graft-0.21.1`):** pinned in `graft-bootstrap.cjs`; `.mcp.json` re-pointed
at `.claude/helpers/graft-mcp.cjs` (ef17d3d had reverted PR #24 — MCP failed to start). An upgrade
or `graft init` rewrites `.mcp.json` — re-point it; check the stamp shape in graft's `dist/upkeep.js`.

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
