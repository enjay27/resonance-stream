# Active State — resonance-stream

**Index, not the record.** At most 40 lines and 6 KB, checked by CI (`bash .github/scripts/memory-check.sh`, decision D-2). Only what would be *false* once stale lives here; the rest is in `.memory/`.

## Now — 2026-10-06

**Where things stand.** `main` has the test bridge's packet-limit fix (#203), reqwest 0.12 (#199), golden tests (#201), weekly mutation tests (#202) and the English review documents (#204-#206). Roadmap stages S0-S7 are in [`.memory/roadmap/refactor.md`](.memory/roadmap/refactor.md). **Done:** S0a-c (#207-#209), S1a-c (#210-#212), S3a (#213), S2c (#214), S2a (#215), S2b-1 (#216), the one-`AppConfig` merge (D-25, #217), S2b-2 (`config_version`, JSON only, D-26, #218), S2d (#219), S3b-1 (#220), S3c-1 (#221), S4a (the second-client duplicate check moved into the pipeline, D-27, #222), S4b (`sniffer_change` + `watchdog_check` in core, #223), S4c (an `Error` state on every socket-setup failure, #224), S4d-1 (`Services` owns the translator, #225), S4d-2 (`Services` owns the sniffer, with a bounded wait for the old capture thread, #226), and S5a (the six `text.rs` mutation gaps closed, #227). **Smoke run 37567018818 (`main` at #226) ended green, all eight pipelines**; only step results read. S5b (W-11 `[P<n>]` collision, #228), S5c (W-12 a leaked role header goes whole, #229), S5d (P-4 + R-7: the config is the one block list, rows emitted after the history lock, #230) and S5e (P-6: the nickname lock is held only to pick the names; this branch): **S5 is complete**. A bridge-smoke dispatch on `main` after #230 is the real-app proof of S5d (read its chat-rules and persistence block rows). **Next:** S6 as Kade approved it (S6a P-1 this branch; then S6b furigana batching, S6c scroll effect, S6d split `ChatRow`), then S7; P-5 and R-9 were skipped by agreement. S3b-2 (signing) and S3c-2 (CSP via `rc`) wait on Kade's answers. One PR at a time. Roadmap and architecture work continues in a new session.

**Written but not verified**
- Smoke run 37545082187 (`main` at #208, 2026-10-06) ended green: all eight pipelines passed, so rows `CS-restart-nodup`, `CP-big-ack`, `CP-fav-*`, popups and download-integrity are read as passing. **Only the step results were read, not the per-row values**: the K8 `<bos>` count and the K16 一人 reading are still Kade's to read from the uploaded `bridge-smoke-logs`. Whether the Node broker (aedes) has a packet limit of its own is open.
- reqwest 0.12 on Kade's PC: translation, dictionary sync and favorites work; the model download was skipped and the update dialog did not show (up to date, or broken? read the system log's update line).
- The app's own tests (`cargo test -p resonance-stream`) run only in Windows CI; here the app is only cross-checked. Smoke runs only from a release tag or by hand, so the rc / release call paths are unproven until the first tag.
- Older things written but never run on Windows: [`active-issues/unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md), sorted by how to check them in [`checklist-triage.md`](.memory/active-issues/checklist-triage.md); the human-only ones are Kade's.

**Open decisions (Kade).** K8 `<bos>` count; K16 how 一人 / 二人 / 一人前 are read; the update-check result (O-1..O-5 in [`docs/decisions.md`](docs/decisions.md)); **S3b-2** how the gist metadata is signed (3 questions at S3b-2 in the roadmap).
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
