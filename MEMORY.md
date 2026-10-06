# Active State — resonance-stream

**Index, not the record.** At most 40 lines and 6 KB, checked by CI (`bash .github/scripts/memory-check.sh`, decision D-2). Only what would be *false* once stale lives here; the rest is in `.memory/`.

## Now — 2026-10-06

**Where things stand.** `main` has the test bridge's packet-limit fix (#203), reqwest 0.12 (#199), golden tests (#201), weekly mutation tests (#202) and the English review documents (#204-#206). Roadmap stages S0-S7 are in [`.memory/roadmap/refactor.md`](.memory/roadmap/refactor.md). S0a (this index) and S0b (smoke steps independent, #208) and S0c (smoke run 37545082187 green) are done; S1a (update swap rolls back, #210), S1b (AI server extracted via `.part`, #211) and S1c (panic log, PR open) too; order agreed with Kade: S3a, S2c, S2a, S2b, S2d, S3b, S3c, S4-S7, one PR at a time. Roadmap and architecture work continues in a new session.

**Written but not verified**
- Smoke run 37545082187 (`main` at #208, 2026-10-06) ended green: all eight pipelines passed, so rows `CS-restart-nodup`, `CP-big-ack`, `CP-fav-*`, popups and download-integrity are read as passing. **Only the step results were read, not the per-row values**: the K8 `<bos>` count and the K16 一人 reading are still Kade's to read from the uploaded `bridge-smoke-logs`. Whether the Node broker (aedes) has a packet limit of its own is open.
- reqwest 0.12 on Kade's PC: translation, dictionary sync and favorites work; the model download was skipped and the update dialog did not show (up to date, or broken? read the system log's update line).
- The app's own tests (`cargo test -p resonance-stream`) run only in Windows CI; here the app is only cross-checked. Smoke runs only from a release tag or by hand, so the rc / release call paths are unproven until the first tag.
- Older things written but never run on Windows: [`active-issues/unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md), sorted by how to check them in [`checklist-triage.md`](.memory/active-issues/checklist-triage.md); the human-only ones are Kade's.

**Open decisions (Kade).** K8 `<bos>` count; K16 how 一人 / 二人 / 一人前 are read; what a leaked `<start_of_turn>model` should become; the update-check result (O-1..O-5 in [`docs/decisions.md`](docs/decisions.md)).
**Decided, not done.** Local and Beginner tab-limit default 1000 (S2c).
**Known defects, not fixed.** A `[P0]` typed in chat collides with a real placeholder; a leaked `<start_of_turn>model` leaves the word "model"; six mutation gaps in `text.rs` (`TranslationCache` eviction order, `is_empty`, `Dictionary` accessors). Pinned by golden tests; see W-11, W-12, A-2.4 in the review.
**Top risks (review).** The self-update has no rollback (W-1); the admin app runs an unchecked `llama-server.exe` (W-2); chat is not archived on some paths (W-4). Stages S1-S3.

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
