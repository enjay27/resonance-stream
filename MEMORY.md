# Active State — resonance-stream

**Index, not the record.** At most 40 lines and 6 KB, checked by CI (`bash .github/scripts/memory-check.sh`, decision D-2). Only what would be *false* once stale lives here; the rest is in `.memory/`.

## Now — 2026-10-07

**New session? Start with [`.memory/sessions/2026-10-07-architecture-roadmap-handoff.md`](.memory/sessions/2026-10-07-architecture-roadmap-handoff.md).**

**Where things stand.** `main` has the test bridge's packet-limit fix (#203), reqwest 0.12 (#199), golden tests (#201), weekly mutation tests (#202) and the English review documents (#204-#206). Roadmap stages S0-S7 are in [`.memory/roadmap/refactor.md`](.memory/roadmap/refactor.md). **Done:** S0a-c (#207-#209), S1a-c (#210-#212), S3a (#213), S2c (#214), S2a (#215), S2b-1 (#216), the one-`AppConfig` merge (D-25, #217), S2b-2 (`config_version`, JSON only, D-26, #218), S2d (#219), S3b-1 (#220), S3c-1 (#221), S4a (the second-client duplicate check moved into the pipeline, D-27, #222), S4b (`sniffer_change` + `watchdog_check` in core, #223), S4c (an `Error` state on every socket-setup failure, #224), S4d-1 (`Services` owns the translator, #225), S4d-2 (`Services` owns the sniffer, with a bounded wait for the old capture thread, #226), and S5a (the six `text.rs` mutation gaps closed, #227). **Smoke runs 37567018818 (`main` at #226) and 37573382046 (at #230) ended green, all eight pipelines**; only step results were read. Then S5b-e (#228-#231: `[P<n>]` collision, leaked role header, one block list, nickname lock scope), S6a-c (#232-#234: list limit shrinks, batched furigana, one scroll effect), a compact-mode click panic fix (#235), and S6d (`ChatRow` split into `chat_row/{mod,menus,ruby}.rs`, #236): **S5 and S6 are complete** (P-5, R-9 skipped by agreement). **Not seen in a real window:** every S6 change (`cargo tauri dev` on Windows, as Administrator). **Next:** S7 (sanitised capture corpus) needs Kade's real capture; S3b-2 (signing) and S3c-2 (CSP via `rc`) wait on Kade's answers. One PR at a time. Roadmap and architecture work continues in a new session.

**Written but not verified**
- Smoke run 37545082187 (`main` at #208, 2026-10-06) ended green: all eight pipelines passed, so rows `CS-restart-nodup`, `CP-big-ack`, `CP-fav-*`, popups and download-integrity are read as passing. **Only the step results were read, not the per-row values**: the K8 `<bos>` count and the K16 一人 reading are still Kade's to read from the uploaded `bridge-smoke-logs`. Whether the Node broker (aedes) has a packet limit of its own is open.
- reqwest 0.12 on Kade's PC: translation, dictionary sync and favorites work; the model download was skipped and the update dialog did not show (up to date, or broken? read the system log's update line).
- The app's own tests (`cargo test -p resonance-stream`) run only in Windows CI; here the app is only cross-checked. Smoke runs only from a release tag or by hand, so the rc / release call paths are unproven until the first tag.
- Older things written but never run on Windows: [`active-issues/unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md), sorted by how to check them in [`checklist-triage.md`](.memory/active-issues/checklist-triage.md); the human-only ones are Kade's.

**Decided 2026-10-07 (Kade).** D-28: model + dictionary metadata is published from this repo's `metadata/`, signed with the app-update key onto a generated `metadata` branch when Kade pushes a tag `metadata-v<N>`; rollback guard on; a bad signature refuses and keeps the last good copy (M1 #237, M2 #238 merged; **`metadata-v1` published 2026-10-07, run green**, branch `metadata` verified with the app's own verifier; a tag must sit on a commit that already has `metadata.yml`). **M3 done and merged (#240 core, #241 app, #242 wizard); bridge-smoke on `main` at #241 green, only step results read, so the `TS-meta-*` row values are unread.** M4 (`download_model` from verified metadata) optional. D-29: the model moves from TranslateGemma to hy-mt2, so K8 is moot. **Waiting on Kade:** he tests the CSP `rc` build and gives the real capture (S7) after he is home; K16 readings from another Claude session; O-3 (update check) is expected to show no dialog while the app is 0.6.1 = the newest release. **Dictionary 1.0.7 (season 3 names from the cheat sheet, 실드 나이트 trees fixed, `마법사` typo fixed) = branch `claude/dict-season3`; after it merges Kade pushes `metadata-v2` on `main`** (old 0.6.x copies read the gist and do not get it).
**Top risks (review), after S1-S3a, S2a, S2c.** Done: update rollback (W-1), `.part` extraction (W-3), panic log (W-5), server-file hashes (W-2; the check-to-spawn window is open), chat archived on arrival (W-4), settings reads and writes reported and versioned (W-7, S2b). `netsh` off the main thread and checked once (W-6, S2d). Left: S3b metadata trust (W-8), S3c webview (W-9), S4-S7.

## Where the detail is

| read | when |
|---|---|
| [`CLAUDE.md`](CLAUDE.md), [`.memory/README.md`](.memory/README.md) | rules, gates, layout; which memory file takes what |
| [`docs/architecture_review.md`](docs/architecture_review.md) | structure, findings, recommendations by feature; also `security_model.md`, `testing.md`, `decisions.md` |
| [`.memory/roadmap/`](.memory/roadmap/), [`.memory/active-issues/`](.memory/active-issues/) | what is next and what proves it done; before trusting a doc, a build or a behaviour |
| [`.memory/sessions/`](.memory/sessions/) | why a decision was made, wrong turns included; the index before the shrink is `2026-10-06-memory-index-archive.md` |

## Rules
- At most 40 lines and 6 KB; CI fails above. *Now* carries what is next and what is unverified; detail goes to `.memory/`.
- Update *Now* every session, even when the answer is "unchanged". Delete an item when it is done; git and `sessions/` keep the history.
