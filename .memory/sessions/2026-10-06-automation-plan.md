# Plan: more automatic test scenarios on the bridge (written 2026-10-06 for a NEW session)

For a session with no context. Read `CLAUDE.md` first (plan first, test first, one PR at a time, the runbook is docs, never report a
gate as passed when it could not run), then `MEMORY.md`, then `.memory/sessions/2026-10-05-bridge-handoff.md` (how the bridge came to be).
Kade asked for this plan; **nothing in it is built**. Kade (the user, Korean, a Windows PC with the game) decides the order; the session
confirms it before building.

## What exists (all on `main` since 2026-10-05/06, PRs #149-#163)

**The idea:** the app, started with `--bridge-url mqtt://127.0.0.1:PORT` in a test build (debug, or `--features test-env`; the rc exe has it),
publishes what it does to a local MQTT broker and obeys *named* test commands. Python defines the scenarios ("pipelines"), Node is the
broker/recorder, a stand-in app proves the pipelines on Linux. Kade's Windows run is the only proof about the real app.

| piece | where |
|---|---|
| wire rules (topics, envelope, command allowlist, acks) -- pure, tested on any OS | `crates/core/src/bridge.rs` |
| the app's MQTT client, event tap (`listen_any`), UI-command tap (`tap_commands`), command handler | `src-tauri/src/bridge.rs` |
| test flags (`--bridge-url`, `--replay-chat`, `--feed-url`, `--metadata-url`, `--data-dir`, `--no-capture` ...) | `crates/core/src/test_env.rs`, `src-tauri/src/test_env.rs` |
| Node: broker + recorder + `expect` / `send`, scenario checks, CLI (`serve`, `run replay-chat`, `verify`) | `runbook/bridge/` (`bridge.mjs`, `scenarios.mjs`, `runner.mjs`, `cli.mjs`) |
| Python: `Serve` (start the broker, `send`, `publish`, `expect`, `expect_sequence`, `events`), `wait_started`, `ensure_installed` | `runbook/runbook/bridge.py` |
| pipelines (one class each, `run(key_path, password)`): `updater-mock`, `interface`, `window-restore`; registered in `runbook/runbook/run.py` | `runbook/runbook/pipelines/` |
| the stand-in app (own minimal MQTT client; models update, window, sniffer line, bugs to catch via `FAKE_APP_*` env) | `runbook/tests/fake_app.py` |
| thin notebooks around the pipelines | `runbook/notebooks/` |

**Topics** (fixed, one broker = one app): `rs/app/event/<name>` (backend -> UI events: `packet-event` = a new chat line, `chat-message-update` = a
blocked-flag re-send, `translation-event`, `system-event`, `sniffer-state`, `translator-state`, `download-progress`, `favorites-changed`, ...; and
the app's own `update-state`, `app-started`), `rs/app/command/<name>` (UI -> backend invokes), `rs/test/command/<name>` (test -> app),
`rs/app/ack/<id>` (`{id, ok, error?, data?}`), `rs/app/status` (`online` / `offline` last will).
**Allowlisted test commands today:** `ping`, `quit`, `replay-chat {path}`, `start-update`, `restart-update`, `grow-window {min_width,min_height}`,
`snapshot`, `close-window`. Rule (Kade, 2026-10-05): **named commands only, never a generic `invoke`**; the broker address must be loopback.

**Run (Windows, Administrator terminal, Node 20+, from `runbook/`):** `python -m runbook.run window-restore | interface | updater-mock --key <backup key file>`;
`node ../runbook/bridge/cli.mjs run replay-chat --exe <exe>`. The exe defaults to `target\release\resonance-stream.exe`
(`cargo tauri build --no-bundle --features test-env`, or a release candidate). Tests: `cd runbook && pytest` (~3 min), `cd runbook/bridge && npm test`.

## How to add a scenario (the recipe -- every earlier one followed it)

1. **Plan with Kade** (impact analysis, which commands/events, what the real app can and cannot show). Wait for his go.
2. **Pure part first** in `crates/core/src/bridge.rs` (test first, see it red): new `Command` variant + validation in `parse_command`, event-name constants.
3. **App part** in `src-tauri/src/bridge.rs` (`handle`) -- a named command calls the same function the UI's command calls; publish extra events with
   `bridge::publish_event`. Cross-check only on Linux: `rustup target add x86_64-pc-windows-gnu`, `apt-get install gcc-mingw-w64-x86-64`,
   `cargo check -p resonance-stream --target x86_64-pc-windows-gnu --tests` and `--release` (never say the app gate passed).
4. **Stand-in** support in `runbook/tests/fake_app.py` (+ a `FAKE_APP_*` switch for the bug the scenario must catch).
5. **Pipeline** `runbook/runbook/pipelines/<name>.py` (copy `window_restore.py`: small, uses `updater.step` for try/finally, `bridge.wait_started`), register it in
   `run.py`, thin notebook (copy `window-restore.ipynb`), README row, `tests/test_pipeline_<name>.py` incl. a case where the stand-in has the bug.
6. **Test the entry point a person types** (`tests/test_run.py` builds every registered pipeline) -- the first Windows run crashed because only the classes were tested.
7. Gates: core `cargo test -p resonance-core -p resonance-llama -p resonance-types`, `cargo fmt --all -- --check`, `cd runbook && pytest`. Memory (`MEMORY.md` Now) updated in the branch.
8. One PR at a time (auto-merge needs green CI), then Kade runs it on Windows; his report decides what is next. Ship app changes to him through the **rc** flow
   (`candidate/<feature>` -> PR into `rc`, merged by hand; the exe is built with `--features test-env`).

## Scenarios, best first

Each has: what it proves | design | new app surface | effort | risk.

1. **Firewall rule per exe (K6)** -- proves the first-run firewall wizard logic and the per-exe rule, today a manual notebook (`firewall.ipynb`). Design: delete the rule for a
   fresh exe copy (`netsh advfirewall`), start with the sniffer on, `expect` `firewall-missing`, `ensure-firewall-rule` (named command -> `ensure_firewall_rule_command`),
   `netsh` shows exactly one rule for that exe path, restart: no `firewall-missing`; clean up the rule at the end (unique exe copy path = unique rule name,
   `resonance_core::sniffer_net` has the naming). New: `ensure-firewall-rule` command. Effort small. Risk: edits the real firewall (admin) -- always clean up, name rules unmistakably.
2. **Chat rules through replay** -- proves the backend's chat handling with no game: `block-user {uid}` (-> `block_user_command`; expect `chat-message-update` with `isBlocked`, later lines
   from that uid dropped), unblock, **dedupe** (the same line twice from two "clients" within 2 s = one `packet-event`; `replay-chat` can carry `client` or duplicate lines),
   `min_sender_level` (WORLD lines below it hidden -- check what is *published* vs *shown*), emote placeholders (already in `replay-chat`), history (`get-chat-history` snapshot ==
   what was published; `clear-history`). New: `block-user`, `unblock-user`, `get-chat-history`, `clear-history` commands (+ replay sample variants in `crates/core/testdata/`). Effort small-medium. Risk low.
3. **Persistence across restart** -- replay -> `quit` -> start again on the same `--data-dir`: the day's chat log file (`chat_logs`) holds the lines, `get_chat_history` reloads them
   (`resonance_core::history::load_recent`), `save-config {patch}` round trip (`snapshot-config` == what was saved; `AppConfig` has two types -- app and ui -- CLAUDE.md says a field
   must exist in both), favorites via `save_favorites` -> `favorites-changed`. New: `snapshot-config`, `save-config`, `get-chat-history`. Effort medium. Risk: config/secret content in snapshots (test builds only).
4. **Download integrity (model / dictionary / server)** -- the downloader's security checks, never tested automatically: a mock gist (`--metadata-url`, the Python `mockfeed.MockServer`
   already serves `/metadata.json`; extend it with `/model.gguf`) -> `download-model` (named) -> `download-progress` order and a 100 % end; a **wrong SHA-256 is refused and removed**, a
   **non-HTTPS / non-local URL is refused** (`resonance_core::download` has the pure checks), a cut or stalled download ends in an error with no half file, cancel works. The updater half already
   exists in `updater-mock` (M5-M7); this is the model/dictionary half. New: `download-model`, `sync-dictionary` commands. Effort medium. Risk low (local mock server).
5. **Synthetic capture (a SPIKE first)** -- the heart of the app, never tested end to end: raw socket -> frame -> protobuf decode -> pipeline -> `packet-event`. Design: build real chat
   frames (reuse the helpers of `crates/core/src/capture/pipeline.rs` tests, e.g. `chat_segment`; put them in an example binary `crates/core/examples/` that prints hex lines, or in
   `crates/core/testdata/`), send them to port 5003 on this PC (a TCP server on 5003 + a client sending the frames), start the app with the sniffer ON and `network_interface` set, expect the
   `packet-event`s. **The open question:** does `SIO_RCVALL` on the LAN adapter's IP see traffic a PC sends to its *own* address (Windows loops it back internally), or must the sniffer bind
   `127.0.0.1`? Try both in a one-PR spike run by Kade before building more. If it works it unlocks: K4/K6 "chat is captured" rows (today `skip`), the **watchdog and VPN hint** (stop sending
   -> `sniffer-state` / the VPN hint after N s; K5), duplicate delivery from two adapters, and a soak test. Effort spike small, full medium. Risk: it may not work -> then document and stop.
6. **Translation with a stand-in llama server** -- the whole translate path with no model: needs a new test flag `--llama-url http://127.0.0.1:PORT` (`crates/core/src/test_env.rs` + the
   translator worker uses it instead of starting `llama-server`), and a stub HTTP server (Python, `/completion` + `/health`) with canned and failing replies. Proves: `translator-state`
   transitions (ready / error / restart: `resonance_core::workers` has the decisions), `translation-event` per line, pre/post-processing (`crates/core/src/text.rs`: dictionary, emotes, romaji),
   stale-job drop, a dead server -> restart, the double-`<bos>` question (K8: assert the prompt the stub *received*). Effort medium-large. Risk: touches the translator wiring.
7. **Popups, tray and shortcuts** -- `open-popup {kind}` + `snapshot` of its rect, `popup-shown`, `--no-popups` prewarm, favorites popup saves only what it owns; tray toggles
   (`tray-toggle-always-on-top`, `-click-through`) and `global-tab-switch` through named commands that call the same handlers. Effort medium. Value moderate (mostly UI glue).
8. **`feed` (K3) without prompts** -- `gh` dispatch of the release-feed workflow and reading the run is already automatic; only K3b ("keys are stored in two places") is a human fact.
   Small: a pipeline with that one row kept as a recorded confirmation.
9. **`checklists` (62 rows from `.memory/active-issues/unverified-on-windows.md`)** -- triage each row: *bridge-checkable* (becomes a scenario above), *screenshot-checkable*, *human only*. Do it with Kade
   once, tick off the ones the earlier scenarios cover. Effort small (a table), saves most.
10. **UI clicks (phase 2, big)** -- the bridge cannot press buttons. Options: (a) `tauri-driver` + Edge WebDriver against the real WebView2 window (real clicks, real Korean text; Windows only,
    flaky setup), (b) keep using the `ui-preview` skill (Playwright against the wasm UI with a mocked Tauri, runs on Linux, already works). Decide only after 1-9; (b) may already be enough for most UI rules.

## QA methods beyond the bridge (Kade asked, 2026-10-06; none built)

Kade's concern: is the bridge a big change to the architecture? Answer given: no -- one module (`src-tauri/src/bridge.rs`, ~300 lines) + a pure one in `crates/core`, one wrapper
around the command handler, one `publish_event` call in the updater; nothing changes in normal runs (starts only with `--bridge-url`, test/debug build, loopback, fixed command list).
**One thing to tighten:** `rumqttc` and the bridge code are compiled into *stable* exes too (never started). Put the bridge entirely behind the `test-env` feature (optional dependency +
`#[cfg(any(debug_assertions, feature = "test-env"))]`; debug builds then need `--features test-env`) so a stable release contains none of it.

| method | catches | architecture impact | effort |
|---|---|---|---|
| bridge behind the `test-env` feature | test code and a dependency in stable exes | small build change | small |
| fuzz / property tests (`proptest`, `cargo-fuzz`) for the protocol decoder and framing | panics / wrong output on malformed bytes from the network | none (tests) | small |
| golden / snapshot tests (`insta`) for the text pipeline (dictionary, emotes, romaji, furigana) | accidental changes to pre/post-processing | none | small |
| config parity test: app `AppConfig` vs ui `AppConfig` keys against one shared fixture | the silent "field added to one side only" break CLAUDE.md warns about | none | small |
| wire-format golden test for `ChatMessage` / `SystemMessage` (camelCase JSON) | a renamed field = a protocol change | none | small |
| `cargo audit` / `cargo deny` in CI | vulnerable or oddly licensed dependencies (the app downloads and runs exes) | none (CI) | small |
| mutation testing (`cargo-mutants`) on `crates/core`, weekly | tests that would not notice a real bug | none | small |
| sanitised capture corpus in the repo, replayed in CI (`crates/core/tests/capture_replay.rs` exists; real captures are kept out of git) | decoder regressions on real traffic | none | small-medium |
| **bridge smoke test in CI** on the rc exe (`windows-latest`: `replay-chat`, `window-restore`) | every rc is checked before Kade opens it | CI only | small-medium; needs a spike: can the runner open the window and is it elevated? (unverified) |
| visual regression: Playwright screenshots of the wasm UI (`ui-preview` skill) | layout breakage | none | medium |
| fault injection over the bridge (network down, llama dies, disk full) | crashes and stuck states in failure paths | new named commands | medium |
| soak test + a read-only `stats` command (memory, handles) | leaks, slow drift | one command | medium |
| in-app "copy diagnostics" (version, config, recent log) | makes tester reports reproducible | small feature | medium |
| real-window UI automation (`tauri-driver` + WebDriver) | actual clicks | heavy, flaky | large -- skip until the rest is done |

My order of preference: bridge behind the feature; bridge smoke test in CI (spike first); fuzz + config-parity + wire-format tests; `cargo audit`; then the scenarios above.

## Suggested order

-1. The bridge behind the `test-env` feature (see the QA table) -- before more app-side commands are added, so every new command lands behind it.
0. Kade's pending Windows runs on the rc build (`window-restore` must now pass after #163; `interface`, `updater-mock`, `replay-chat` passed on 2026-10-06 with an older build). Fix what they show first.
1. **Spike 5 (synthetic capture)** in parallel with the sure ones: it is a one-PR experiment whose answer decides items 5 and the "captured" rows -- ask Kade to run it early.
2. Then 1 (firewall), 2 (chat rules), 3 (persistence), 4 (download integrity), in that order -- each is small and reuses existing commands.
3. Then 5 in full, 6, 7; 8 and 9 whenever Kade has ten minutes.

## Gotchas learned (read before you start)

- A new chat line is **`packet-event`**; `chat-message-update` is only a blocked-flag re-send. (The first `replay-chat` check read the wrong one.)
- Test the command line a person types, not only the classes (`run.py` crashed on the first Windows run).
- A silent wait looks like a hang: pipelines print what they wait for (`bridge.wait_started`) and show the app's log on timeout.
- Kade's Windows checkout can be stale or his exe an old build (it caused two false alarms): ask him to `git checkout -- runbook/notebooks`, `git pull`, rebuild with `--features test-env` -- or use the rc exe.
- `window.rs`: `set_size` sets the INNER size, the remembered rects are OUTER (the K18 pipeline found a +22x+13 px drift per restore; fixed in #163 with `inner_size_for`).
- The backup signing key is a **file + its password** (asked once per exe; the signature is cached in `runs/mock/signed/`); the primary key stays out of tests.
- `pkill -f <pattern>` kills your own shell when the pattern is in its command line: kill by pid.
- Reusing names: the stand-in app (`fake_app.py`) is only a reading of the app; a green dry run proves the pipeline, never the app.
- One PR at a time; subscribe (`subscribe_pr_activity`) and wait for the merge notice, do not poll. Memory (`MEMORY.md` Now) goes in the same branch.

## Questions to put to Kade at the start

1. Order: spike 5 first (my recommendation, one cheap experiment), or the sure ones first?
2. New test flag `--llama-url` (item 6) -- OK to add a flag that lets a test replace the translator server?
3. Item 1 edits the real Windows firewall (admin; always cleaned up) -- OK, or keep K6 manual?
4. Item 10: is UI click automation wanted at all, or is `ui-preview` enough?

## Prompt for the new session

> Read `CLAUDE.md`, `MEMORY.md`, `.memory/sessions/2026-10-06-automation-plan.md` and `.memory/sessions/2026-10-05-bridge-handoff.md`. I am Kade. We add more automatic test
> scenarios on the bridge. Start by asking me the four questions in the plan and showing me the results of my last Windows runs (I will paste them); then present the impact
> analysis for the first scenario and wait for my go.
