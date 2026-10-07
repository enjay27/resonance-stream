# Active State — resonance-stream

**Index, not the record.** At most 40 lines and 6 KB, checked by CI (`bash .github/scripts/memory-check.sh`, decision D-2). Only what would be *false* once stale lives here; the rest is in `.memory/`.

## Now — 2026-10-07

**New session? Start with [`.memory/sessions/2026-10-07-open-items-n2-n5-handoff.md`](.memory/sessions/2026-10-07-open-items-n2-n5-handoff.md)** (the earlier handoffs are background).

**Where things stand.** Roadmap stages S0-S6 and the signed metadata (M1-M3) are done and merged (#207-#249); the before/after is [`docs/refactoring_report.md`](docs/refactoring_report.md), the stages and the open items found after S6 (N-1 to N-5) are in [`.memory/roadmap/refactor.md`](.memory/roadmap/refactor.md). **Real window (Kade, 2026-10-07):** S6 and the signed first-run path confirmed; not yet the wizard's refusal line, the update check (O-3), or the new dictionary terms in real chat. **Next:** N-2, N-5 (N-3, N-4, N-6, N-1 done; N-6 = the settings line "v1.0.8 · 서명 리비전 N · 게시본과 같음/직접 수정됨/확인 불가", not yet seen in a real window), one PR at a time (gist untouched); the CSP PR #248 into `rc` is Kade's to merge and test; S7 and hy-mt2 wait for his inputs (roadmap). One PR at a time.

**Claude config refactor** (plan: `enjay27/claude-skills` `docs/refactor-plan.md` §4). R1 (rules moved to `.claude/rules/` + `release` skill) and R2 (`CLAUDE.md` 99 lines, imports `MEMORY.md`) done. Next R3, R4.

**Written but not verified**
- Smoke runs on `main` at #208, #226, #230, #236 and #241 (2026-10-06/07) all ended green, all eight pipelines, so rows like `CS-restart-nodup`, `CP-big-ack`, `CP-fav-*`, popups, download-integrity and `TS-meta-*` are read as passing. **Only the step results were read, not the per-row values**: the K8 `<bos>` count (moot, D-29), the K16 一人 reading and the `TS-meta-*` values are Kade's to read from the uploaded `bridge-smoke-logs`. Whether the Node broker (aedes) has a packet limit of its own is open.
- reqwest 0.12 on Kade's PC: translation, dictionary sync and favorites work; the model download was skipped and the update dialog did not show (up to date, or broken? read the system log's update line).
- The app's own tests (`cargo test -p resonance-stream`) run only in Windows CI; here the app is only cross-checked. Smoke runs only from a release tag or by hand, so the rc / release call paths are unproven until the first tag.
- Older things written but never run on Windows: [`active-issues/unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md), sorted by how to check them in [`checklist-triage.md`](.memory/active-issues/checklist-triage.md); the human-only ones are Kade's.

**Decided 2026-10-07 (Kade).** D-28: model + dictionary metadata is published from this repo's `metadata/`, signed with the app-update key onto a generated `metadata` branch when Kade pushes a tag `metadata-v<N>`; rollback guard on; a bad signature refuses and keeps the last good copy (M1 #237, M2 #238 merged; **`metadata-v1` published 2026-10-07, run green**, branch `metadata` verified with the app's own verifier; a tag must sit on a commit that already has `metadata.yml`). **M3 done and merged (#240 core, #241 app, #242 wizard); bridge-smoke on `main` at #241 green, only step results read, so the `TS-meta-*` row values are unread.** M4 (`download_model` from verified metadata) optional. D-29: the model moves from TranslateGemma to hy-mt2, so K8 is moot. **Waiting on Kade:** he gives the real capture (S7) after he is home; K16 readings from another Claude session; O-3 (update check) is expected to show no dialog while the app is 0.6.1 = the newest release. **Dictionary 1.0.7 (season 3 names, 실드 나이트 trees, `마법사` typo; #243) is published: `metadata-v2` run green 2026-10-07, revision 2 checked with the app's verifier** (old 0.6.x copies read the gist and do not get it). **Dictionary 1.0.8 (Kade: cheat sheet's names win, no new one-character keys): the eight class trees and イサキ now follow the cheat sheet; Kade pushes `metadata-v3` after the merge.** CSP merged to `main` (#251, tested in the rc exe).
**Top risks (review), after S1-S3a, S2a, S2c.** Done: update rollback (W-1), `.part` extraction (W-3), panic log (W-5), server-file hashes (W-2; the check-to-spawn window is open), chat archived on arrival (W-4), settings reads and writes reported and versioned (W-7, S2b). `netsh` off the main thread and checked once (W-6, S2d). Left: S3b metadata trust (W-8), S3c webview (W-9), S4-S7.

## Where the detail is

| read | when |
|---|---|
| [`CLAUDE.md`](CLAUDE.md), [`.memory/README.md`](.memory/README.md) | rules, gates, layout; which memory file takes what |
| [`docs/architecture_review.md`](docs/architecture_review.md) | structure, findings, recommendations by feature; also `security_model.md`, `testing.md`, `decisions.md`; the result, before/after: `refactoring_report.md` |
| [`.memory/roadmap/`](.memory/roadmap/), [`.memory/active-issues/`](.memory/active-issues/) | what is next and what proves it done; before trusting a doc, a build or a behaviour |
| [`.memory/sessions/`](.memory/sessions/) | why a decision was made, wrong turns included; the index before the shrink is `2026-10-06-memory-index-archive.md` |

## Rules
- At most 40 lines and 6 KB; CI fails above. *Now* carries what is next and what is unverified; detail goes to `.memory/`.
- Update *Now* every session, even when the answer is "unchanged". Delete an item when it is done; git and `sessions/` keep the history.
