# Handoff -- automatic tests on the bridge, after the small rows (2026-10-06)

For a new session with no context. Read `CLAUDE.md` (plan first, test first, one PR at a time, the runbook is docs, never report a gate as passed
when it could not run), `MEMORY.md`, then [`../roadmap/test-automation.md`](../roadmap/test-automation.md) (the status list) and
[`2026-10-06-automation-plan.md`](2026-10-06-automation-plan.md) (the recipe for a new command, the gotchas). Kade is the user (Korean, a Windows PC
with the game).

## Where things stand

Merged this session, one PR at a time (`claude/*`, auto-merged):

| PR | what | on the real app (hosted `windows-latest`, `bridge-smoke.yml`) |
|---|---|---|
| #187 | the smoke test gates a **stable release only**; a release candidate is published whatever it said and its notes carry the result (`rc_smoke_note`) | not run yet: the first rc / release run is the test (the rc workflow is read from the `rc` branch, so `main` must be merged into `rc` first) |
| #188 | `persistence` + CP-version, CP-retention, CP-retention-served, CP-busy-world | green |
| #189 | `translator-stub` + TS-log-quiet, TS-reload, TS-later-wait / -catchup, TS-live-first, TS-hang | green |
| #190 | `capture-spike` + burst, bytes, CS-watchdog, CS-log-dedup, CS-vpn-hint, CS-restart-nodup | burst / bytes / watchdog / log dedup / VPN hint green; **CS-restart-nodup died on a 10 s ack timeout** (below) |
| `claude/capture-restart-waits-for-ready` | the fix: the restart row waits up to 90 s for its first ack (`READY_WAIT_S`); stand-in `FAKE_APP_READY_DELAY`; test red first | **read its smoke run first** |

**The #190 finding, so it is not repeated:** the app says `app-started` early, but the bridge holds every command until start-up is finished
(`bridge/live.rs`, up to 60 s, `test_env::is_ready`), and a start with the sniffer on takes ~20 s on the runner. `Serve.send` defaults to 10 s.
After a **restart with capture on**, give the first command `timeout=READY_WAIT_S`. A failed pipeline fails the whole smoke job and the later
steps (chat-rules, persistence, download-integrity, translator-stub, popups) do not run in that job, so a red capture-spike hides the rest.

Real-app facts learned on the way (all from smoke runs): the runner is elevated, interactive, 1024x720; its route adapter is not classified
virtual (the VPN hint is absent on both log and badge: they agree); the watchdog writes its log line at ~20 s and ~80 s (`(repeated 2 more
times)`); the sniffer keeps up with a 500-frame write and 20 one-byte segments on both addresses. The watchdog and the throttle windows are the app's
(15 s trip, 60 s hold).

Two rows moved on purpose: "no duplicates after a restart" went from persistence to capture (`--replay-chat` builds its own `ChatPipeline::new()` and
never `remember`s the restored chat; only the real sniffer does, `sniffer/mod.rs:184`); "translations reload once" went to translator-stub.

## What is next: the command rows (app code -- plan first, wait for Kade's go)

Each is a new named bridge command: core `Command` variant + `parse_command` rule + parse tests (red first) in `crates/core/src/bridge.rs`, one
match arm in `src-tauri/src/bridge/live.rs` that calls the function the UI's invoke reaches, a stand-in model with a bug switch in
`runbook/tests/fake_app.py`, rows in an existing pipeline, README / notebook / memory. One PR each, in this order (Kade has not yet said go):

| # | commands | proves | notes |
|---|---|---|---|
| A | `restart-sniffer` | the sniffer comes back (Pending/Active) and new frames arrive | handler calls `restart_sniffer_command(app)` (`sniffer/mod.rs`, spawns a thread; ack at once, wait for `sniffer-state`). The new worker is taught the history (`remember`), so resend **new** texts, not the old frames. Row goes in `capture-spike` |
| B | `save-favorites {favorites}`, `get-favorites` | favorites survive a restart; `favorites-changed` fires; another config write (`block-user`) does not overwrite them; tab ids stay stable | `save_favorites` is `config/app_config.rs:266`; CLAUDE.md: favorites change only through it |
| C | `annotate-furigana {texts}` | the spans the built exe gives; answers roadmap K16 (CI's dictionary: 一人 as イチ ニン?) | tiny wiring around a pure function |
| D | `sync-dictionary`, `save-local-dictionary` | sync applies the dictionary, the next prompt carries the term; `auto_sync_latest_dict` off (default) syncs nothing at start; a local edit applies without a restart | biggest: `mockfeed` needs a dictionary endpoint (it already serves `/metadata.json`) |

Gate per PR: `just core-check` (Linux); the app part can only be **cross-checked** here (`rustup target add x86_64-pc-windows-gnu`, `apt install
gcc-mingw-w64-x86-64`, then `cargo check -p resonance-stream --target x86_64-pc-windows-gnu --tests` with and without `--features test-env`, and
`--release --features test-env`) -- say "NOT VERIFIED: app gate" in the commit body and let Windows CI run it. The runbook part: courtesy
`cd runbook && python -m pytest -q` (about 10 minutes now). Then read the PR's smoke run on the real app.

Also open, from the roadmap: reqwest 0.11 -> 0.12 (the `cargo audit` finding, waits for Kade), K8 `<bos>`, QA methods not built (golden tests with
`insta`, `cargo-mutants`, a sanitised capture corpus, visual regression, fault injection, soak + `stats`, in-app "copy diagnostics").

## Working notes for the session (things that cost time)

- **Never `pkill -f` / `xargs kill` with a pattern that is in your own command line**: it killed the shell twice. Kill by pid (`ps -eo pid,args`).
- A fresh sandbox needs `pip install pytest nbformat nbclient ipykernel` and `npm ci` in `runbook/bridge/`.
- Start long test runs with `nohup ... > /tmp/claude-0/x.log 2>&1 &` and poll the log; two runs writing one log file mix their output.
- A stand-in app bug case that makes the app crash costs a 120 s `wait_started`; give new waits a module constant (`WAIT_S`) the test patches down.
- `mcp__github__actions_list` ignores `per_page` and returns every run (huge); use `pull_request_read get_check_runs` and `get_job_logs` with
  `tail_lines` instead.
- Memory notes for a PR go into that PR's own branch; a merged PR's smoke result lands in the next PR.
