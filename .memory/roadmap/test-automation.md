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
   - [x] `PP-place` fixed (#184): the real app now restores the popup at 520x660 (smoke run of #184: popups 8/8; inner 504x651 before and after).
   - [x] `translator-stub`: UTF-8 fixes (#183 read, #185 print); the real app now passes **6/6** (translate, prompt, restart, catch-up; K8 `<bos>` count 1 reported).
1. [~] **Smoke is a gate for a stable release only** (`claude/smoke-gates-rc-and-release`, then `claude/smoke-gate-release-only`, Kade 2026-10-06: "make release only"): no `continue-on-error` left in `bridge-smoke.yml`; `release.yml` runs it on a separate test-flag build of the tagged commit and `release` needs it.
   `release-candidate.yml` still runs it on the exe it built (input `exe-artifact`) but **only as a report**: the candidate is published whatever it said, and the notes carry a warning (red) or a line (green / not run) -- `rc_smoke_note`. Why: a candidate exists to be tried on Windows; a runner-only failure must not withhold it. `rc` is never merged into `main`, and candidate PRs into `main` are merged by hand, so there is no `rc` -> `main` gate to add (a required check there would be a branch-protection setting, Kade's).
   The call paths (`workflow_call`) cannot run before merge: **the first rc / release run is their test**; a red release run only blocks that publish (re-run). The `push`-triggered rc workflow is read from the `rc` branch: this change takes effect once `main` is merged into `rc`.
2. **Small rows** (no app change; another row in an existing pipeline), three PRs in this order:
   - [x] `persistence` (#188): **CP-version**, **CP-retention** + **CP-retention-served**, **CP-busy-world** built; each caught by a stand-in bug switch (`no-retention`, `retention-takes-all`, `global-limit`, a wrong `FAKE_APP_VERSION`). **Real app: green** (the smoke run of #188).
   - [~] `translator-stub` (`claude/translator-small-rows`): **TS-log-quiet** (no `Polling` line in the system log), **TS-reload** (each Japanese line served once, translated, after a restart), **TS-later-wait / TS-later-catchup** (translation off, then `start-translator`: the newest `translation_catch_up_limit` only, oldest first), **TS-live-first** (slow server still loading; a live line said during the catch-up goes before its last item), **TS-hang** (a request never answered costs one line, no restart; waits the app's 30 s). Stub got `completion_delay` and a `hang` mode. Real app: read the smoke run. The `Polling` line is a `log::trace!` in the app, so TS-log-quiet watches the *system log* only.
   - [~] `capture-spike` (`claude/capture-small-rows`): **CS-<variant>-burst** (500 frames in one write) and **CS-<variant>-bytes** (20 frames, one byte per TCP segment) on both addresses; on the first address only: **CS-watchdog** (state Error after the chat stops), **CS-log-dedup** (100 s of silence -> the log line twice, the second with `(repeated N more times)`), **CS-vpn-hint** (log and badge agree; the runner's Hyper-V route is virtual, so both should name it), **CS-restart-nodup** (the same frames sent again after a restart: an archived line is not shown twice). 1-byte segments are 20 frames, not 100: thousands of tiny packets are enough to prove the reassembly and a busy runner must keep up. Adds about 4 minutes to the smoke run. Real app: read the smoke run of the PR.
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
