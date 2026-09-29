# Active State — resonance-stream

**Index, not the record.** Only what would be *false* the moment it goes stale lives here.

## Now — 2026-09-29 (refactor done, pushed, CI green)

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

**Graft MCP start-up fix merged (PR #24, cee5842).** `.mcp.json` runs `.claude/helpers/graft-mcp.cjs`,
which waits for the cloud install. Upgrading graft past 0.20.0 rewrites `.mcp.json` — re-point it.

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
