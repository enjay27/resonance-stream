# Active State — resonance-stream

**Index, not the record.** Only what would be *false* the moment it goes stale lives here.

## Now — 2026-09-29 (refactor done, unpushed)

**Graft-guided refactor, phases 0–5: done** —
[`.memory/roadmap/refactor-2026-09.md`](.memory/roadmap/refactor-2026-09.md).
Nothing pushed (push is Kade's). **CI has never run** — the first push runs it, and the
Windows job is the first ever automated build + test of `src-tauri/`.
Open decisions for Kade: `AppConfig` unification, unused `pcap`/`windivert` deps,
TROUBLE_SHOOTING rewrite, the "v2.0" footer.

**The app part (`src-tauri/`) only cross-*checks* on Linux** (`just app-cross-check`,
mingw) — it cannot link, so its tests and any run need Windows (CI or Kade's machine). Anything that
skipped it is listed in
[`.memory/active-issues/unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md).

**`AppConfig` is still defined twice** (ui has derived `Default`, app a hand-written one) —
[`.memory/active-issues/duplicate-appconfig.md`](.memory/active-issues/duplicate-appconfig.md).
**Docs:** BUILD/README_EN fixed; TROUBLE_SHOOTING flagged —
[`.memory/active-issues/stale-docs.md`](.memory/active-issues/stale-docs.md).

**Graft MCP start-up fix is PR #24, open.** Until merged, cloud sessions get the CLI only.

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
