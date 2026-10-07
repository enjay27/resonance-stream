# Active State — resonance-stream

**Index, not the record.** At most 40 lines, 6 KB and 200 characters a line, checked by CI (`memory-check.sh`, D-2).
Only what would be *false* once stale lives here; the rest is in `.memory/`.

## Now — 2026-10-08

- **Where:** stages S0–S6 and signed metadata M1–M3 merged; open items: [`roadmap/refactor.md`](.memory/roadmap/refactor.md), "Open items found after S6".
- **Next:** N-2, then N-5, one PR at a time; each changes a wire or a pipeline: plan first, wait for Kade. Steps: [n2-n5 handoff](.memory/sessions/2026-10-07-open-items-n2-n5-handoff.md).
- **Claude config refactor** (`enjay27/claude-skills` `docs/refactor-plan.md` §4): R1, R2, R3a (*Now* shrink), R3b (CI checks) done; next R4 context-guard.
- **Waiting on Kade:** push `metadata-v3` (dictionary 1.0.8); a stable release (also the only proof of O-3); S7 capture; hy-mt2 inputs (D-29).
- **Kade, in a real window:** N-6 and N-1 (the line under the sync button), the wizard's refusal line, the new class-tree names in chat.
- **K16 and `TS-meta-*` values:** artifact `bridge-smoke-logs` of run 37613640483 **expires 2026-10-14**; after that, re-run `bridge-smoke.yml` on `main`.

**Written but not verified**
- Smoke runs on `main` (#208–#241) green, step results only; per-row values unread (K16, `TS-meta-*`). Detail: `roadmap/refactor.md`.
- reqwest 0.12 on Kade's PC: model download and update dialog not seen (read the system log's update line). Detail: `roadmap/refactor.md`.
- rc and release call paths unproven until the first tag (smoke runs only from a tag or by hand).
- Bridge broker (aedes) packet limit: open. Older Windows-only items: [`active-issues/unverified-on-windows.md`](.memory/active-issues/unverified-on-windows.md).

**Decided** (detail in `docs/decisions.md`): D-28 signed metadata (`metadata-v1`, `-v2` published), D-29 model → hy-mt2 (K8 moot).
**Stages and risks left:** `roadmap/refactor.md` (S3–S7, W-items).

## Where the detail is

| read | when |
|---|---|
| [`CLAUDE.md`](CLAUDE.md), [`.memory/README.md`](.memory/README.md) | rules, gates, layout; which memory file takes what |
| [`docs/architecture_review.md`](docs/architecture_review.md) | structure, findings, recommendations by feature |
| `docs/`: `decisions.md`, `security_model.md`, `testing.md`, `refactoring_report.md` | decisions log; trust; tests; before/after |
| [`.memory/roadmap/`](.memory/roadmap/), [`.memory/active-issues/`](.memory/active-issues/) | what is next and what proves it done; before trusting a doc, a build or a behaviour |
| [`.memory/sessions/`](.memory/sessions/) | why a decision was made, wrong turns included; the index before the shrink is `2026-10-06-memory-index-archive.md` |

## Rules
- At most 40 lines, 6 KB, 200 characters a line; CI fails above. *Now* carries what is next and what is unverified; detail goes to `.memory/`.
- Update *Now* every session, even when the answer is "unchanged". Delete an item when it is done; git and `sessions/` keep the history.
