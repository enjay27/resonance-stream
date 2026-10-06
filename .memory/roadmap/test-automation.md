# Roadmap -- automatic tests on the bridge (status 2026-10-06, after PRs #167-#181)

Source plan: [`../sessions/2026-10-06-automation-plan.md`](../sessions/2026-10-06-automation-plan.md) (the recipe, the gotchas).
What was built: [`../sessions/2026-10-06-automation-changelog.md`](../sessions/2026-10-06-automation-changelog.md).
Triage of the manual checklist: [`../active-issues/checklist-triage.md`](../active-issues/checklist-triage.md).
Tick `[x]` only when it ran the way "done when" says; a smoke row that is `continue-on-error` is not a gate yet.

## Plan items (the plan's numbers)

- [x] **-1. Bridge behind `test-env`** (#167).
- [ ] **1. Firewall rule per exe (K6)** -- **stays manual** (Kade, 2026-10-06). The capture-spike makes a rule only with `--add-firewall-rule`.
- [x] **2. Chat rules** (#174), **3. Persistence** (#175, found + fixed a real bug in #177), **4. Download integrity** (#176) -- model half only; the dictionary half is under "Next" 3.
- [x] **5. Synthetic capture** (#173): the sniffer **does** see this PC's own traffic (LAN address and 127.0.0.1). Not done from it: the watchdog / VPN hint rows (K5), duplicate delivery from two adapters, a soak run.
- [~] **6. Stand-in llama server** (#178, #181): `--llama-url`, `llama_stub.py`, `translator-stub`. Built and `TS-ready` passes on the real app; the other rows have not run there yet (finding 0). K8 (`<bos>`) is *reported*, not judged.
- [~] **7. Popups** (#180): 7 of 8 rows pass on the real app; `PP-place` found a bug (finding 0) -- tray and global shortcuts left out (they are events from real clicks / key presses).
- [x] **8. `feed` (K3)** -- nothing to build: automatic apart from K3b, a human fact.
- [x] **9. Checklist triage** (#179).
- [ ] **10. UI click automation** -- **not wanted for now** (Kade, 2026-10-06). `ui-preview` stays the UI check.

## Next, in this order

0. **Two findings from the last smoke runs** (details in the changelog, "Found, not fixed"):
   - [~] `PP-place`: restored popup 30 px taller (left 520x660, back 520x690; the plugin saves/restores the INNER size, `set_size` on the hidden
     undecorated window inflates it). `claude/popup-restore-size`: `show_popup` restores again once shown; the snapshot now reports `inner`. Not unit-testable
     (Tauri glue): **the smoke run is the test** -- if `PP-place` is still red, read its `inner` numbers before a second attempt.
   - [x] `translator-stub` crash on Windows: `bridge.py` now reads its child processes as UTF-8 (`claude/bridge-utf8-pipes`, test first). The
     `TS-*` rows after `TS-ready` have still to be read from the next smoke run on the real app (first real look at translate / restart / catch-up).
1. **Make the proven smoke steps gates.** One PR on `.github/workflows/bridge-smoke.yml`: drop `continue-on-error` from every
   pipeline step that passed on the real app in a run of `main` (capture-spike 5/5, chat-rules 7/7, persistence 7/7, download-integrity 8/8 are
   proven; translator-stub and popups once finding 0 is fixed and their rows are green). Until then a green smoke run can hide a red step.
   Consider also making the rc build (`release-candidate.yml`) call it (`workflow_call` is already there).
2. **Small rows** (no app change; another row in an existing pipeline) -- the list is in the triage: retention-days prune,
   busy-WORLD reload, translation-ledger "turn on later", log dedup, version row, burst frames, the K5 watchdog / VPN-hint rows
   (the capture spike now makes them possible).
3. **Command rows** (a new named command each, recipe in the plan): `restart-sniffer`, `save-favorites` / `get-favorites`,
   `sync-dictionary` / `save-local-dictionary` (the dictionary half of download integrity), `annotate-furigana`.
4. **QA methods not built yet** (plan table): golden tests for the text pipeline (`insta`), mutation testing
   (`cargo-mutants`, weekly), a sanitised capture corpus replayed in CI, visual regression with `ui-preview`, fault injection
   over the bridge, soak + a read-only `stats` command, in-app "copy diagnostics".
5. **Decisions that wait on Kade**
   - reqwest 0.11 -> 0.12 (the `cargo audit` finding from #172; K24 said "dropped by design, revisit only if asked" -- it was asked
     by the audit; **unanswered**). Needs the app gate on Windows.
   - K8 `<bos>`: the stand-in reports the literal `<bos>` count of every prompt; the answer still waits for resonance-lab.
   - Whether the rc flow should run `bridge-smoke` before a candidate is published.

## Facts to keep

- Real-app results are only from the hosted runner (`windows-latest`, elevated, interactive session, 1024x720). It is not Kade's PC:
  GPU, the game, the tray, DPI scaling and anti-cheat are still his.
- Every pipeline's stand-in model has one bug switch per rule. A green dry run proves the pipeline, never the app.
- The app gate (`cargo test -p resonance-stream`) runs only in CI's Windows job; a Linux session can only cross-check (compile).

## Prompt for the next session

> Read `CLAUDE.md`, `MEMORY.md`, `.memory/roadmap/test-automation.md` and `.memory/sessions/2026-10-06-automation-changelog.md`. I am Kade.
> Start with "Next" 0 (the two findings from the smoke runs), one PR at a time, test first; then 1. Show me the impact analysis before changing code.
