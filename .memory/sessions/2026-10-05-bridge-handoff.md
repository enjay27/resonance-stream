# Handoff: the test bridge (planned, nothing built) and the state after Kade's Windows runs (2026-10-05)

For a **new session** with no context. Read `CLAUDE.md` first (especially *Guardrails: plan first*, *The runbook is
docs*, *Version Control*: one PR at a time), then `MEMORY.md`. Kade will plan the bridge with you; **nothing of it is
built, no file was changed for it.** He asked: "make bridge in backend-frontend feature or networking, and then run
bridge to confirm each test's expected action. Flow: 1. run bridge server (Node or Python), listen to the app's
messages (JSON); 2. the app sends a message when an action occurred; 3. the bridge verifies the action."

## The proposal made to Kade (he has not answered; defaults below are *proposed*)

- **Protocol:** the app, started with `--bridge-url http://127.0.0.1:PORT` (and `RESONANCE_TEST_BRIDGE_URL`), POSTs one JSON
  object per action: `{"seq", "t_ms", "kind": "event" | "command", "name", "payload"}`. Sent from a background thread
  with a queue: a slow or missing bridge never blocks the app.
- **Gate:** the same as the other test flags (debug builds and `--features test-env`; stable releases never read it).
  Payloads hold chat text, so test builds and local addresses only.
- **Two taps, no edits at the emit sites:**
  1. backend -> UI events: every one goes through `app.emit(...)`: `chat-message-update`, `sniffer-state`,
     `translator-state`, `system-event`, `packet-event`, `favorites-changed`, `firewall-missing`, `global-tab-switch`,
     `tray-toggle-always-on-top`, `tray-toggle-click-through` (grep `\.emit\("` in `src-tauri/src`). Tauri's
     `listen_any` receives them: one list of names in one new module.
  2. UI -> backend commands: wrap the single `invoke_handler(generate_handler![...])` in `src-tauri/src/lib.rs` to log
     the command name and arguments (`grow_window`, `save_config`, `start_sniffer_command`, ...). Check this wrapper
     is possible in Tauri 2 before promising it (it is the part not yet verified).
- **The bridge server:** Python standard library, `runbook/runbook/bridge.py` (the runbook is Python): collects events,
  writes a JSONL log, offers `expect(name, where=..., timeout=...)` and `expect_sequence([...])`. Notebooks use it instead of
  polling `--status-file` (which stays; the bridge is its push version).
- **Scenarios to verify with it:** `--replay-chat` (eight `chat-message-update` events with the right channel, level
  and emote text, then a `system-event` "Replay finished"); sniffer state changes (incl. the VPN hint of the watchdog);
  the mock-feed updater's update states in order (today `test_env::report_update` writes the status file); K18 (a
  `grow_window` command, then the restore on exit).
- **One-way** (app -> bridge). Whether the bridge should also drive the app (send commands back) is a question for Kade.
- **PR order, one at a time:** (1) `--bridge-url` flag in `crates/core::test_env` (pure, test first); (2) the tap in
  `src-tauri` (new `bridge.rs`, small edits in `lib.rs` and `test_env.rs`; cross-check only, Windows CI decides);
  (3) `bridge.py` + tests + the `replay-chat` scenario against a stand-in app (extend `runbook/tests/fake_app.py`), then
  the other scenarios and notebooks one by one.
- **What cannot be verified in a Linux session:** the real app. Run the bridge against a stand-in here; Kade's Windows run
  is the proof (say `NOT VERIFIED` until then).
- **Questions for Kade:** transport (HTTP POST proposed; WebSocket needs a new dependency), Python vs Node (Python
  proposed), one-way or two-way, which scenario first.

Existing pieces to build on: `crates/core/src/test_env.rs` + `src-tauri/src/test_env.rs` (flags, status file, `mark_ready`,
`report_update`), `reqwest` already in `src-tauri`, `runbook/runbook/mockfeed.py` (mock server, `flag_args`, `wait_status`).

## State after Kade's Windows runs (all of it is in `MEMORY.md` and the roadmap too)

- Runbook results: `updater-mock` 45/0, `firewall` (K6) 8/0, `feed` (K3, K3b) all pass, `checklists` 62/0 (**he pasted only
  the count: ask for the table before deleting bullets from `unverified-on-windows.md`**), `interface` K4-1 pass.
- **K4-2 / issue [#142](https://github.com/enjay27/resonance-stream/issues/142):** with a full-tunnel VPN (NordVPN, adapter
  `NordLynx`) chat is captured on neither the VPN nor the physical adapter. The adapter pick was fixed (#141), the watchdog
  now names the VPN (#144); the capture itself is unsolved and **Kade has not decided** (document only, or investigate).
- Merged today: #138 (gh colour codes), #139 / #140 (`replay-chat` notebook; its sample's WORLD lines had level 0 and
  `min_sender_level` defaults to 1), #141, #143 (roadmap), #144 (K5 hint), #145 (nav bar tabs vs buttons), #146 (K13),
  #147 (K18).
- **Waiting on Kade (Windows):** look at the narrow window (#145); K18 check (open Settings, quit by the tray and by the X,
  restart: old size); `replay-chat` re-run; `interface` K4-3 (offline) and K4-2 on a build with #141 / #144 (expect
  `(first physical adapter)` and, with the VPN on, the VPN hint in the badge).
- **resonance-lab** (read-only, `/home/user/enjay27/resonance-lab` if cloned again with `add_repo`): it owns the translation
  prompt; the app's model is to be replaced (Hy-MT2-1.8B). K8 (`<bos>`), K10 wait on it. Its `stream-contract.md` says
  LLaMA-Factory adds BOS once in training, so the app's literal `<bos>` is very likely a double BOS.
- Roadmap left: K8, K10 (lab), K12, K19 (Kade's answers), K9 (needs a capture), K14, K16, K17, K21-K24.

## Habits that paid off, and mistakes to avoid

- Test first, and look at the red: twice the failing test caught a real design slip (the held-translation map reset by
  `mem::take`; a sample whose WORLD lines were hidden by design).
- Install the cross-check once: `rustup target add x86_64-pc-windows-gnu` and `apt-get install gcc-mingw-w64-x86-64`, then
  `cargo check -p resonance-stream --target x86_64-pc-windows-gnu --tests` (and `--release`). Never say the app gate passed.
- `pkill -f <pattern>` kills your own shell when the pattern is in its command line: kill by pid.
- One PR at a time; subscribe to the PR (`subscribe_pr_activity`) and wait for the merge notice, do not poll.
- `ui-preview`: `preview.sh` serves on :8765 and keeps running; kill the `http-server` pid when done.

## Prompt for the new session

> Read `CLAUDE.md`, `MEMORY.md` and `.memory/sessions/2026-10-05-bridge-handoff.md`. I am Kade. We plan the test bridge
> (the app POSTs JSON for each action to a local Python server that verifies it). Start by confirming the four questions in
> the handoff with me; then present the PR 1 plan (the `--bridge-url` flag) and wait for my go.

## Decisions (Kade, later the same day) -- these replace the proposal above where they differ

- **MQTT, two-way, Node.** Not HTTP POST, not Python. Node `aedes` broker + `mqtt` client in `runbook/bridge/`; the app is an MQTT client (`rumqttc`, only with debug / `test-env`). Broker address must be loopback.
- **Topics**, namespace `rs/<runId>/`: `app/event/<name>` (backend -> UI events), `app/command/<name>` (UI -> backend commands), `test/command/<name>` (test -> app, allowlisted: emit a chat message, invoke a command, replay-chat, quit), `app/ack/<id>`, `app/error`.
- **First scenario:** `replay-chat`.
- **PR order:** (1) `--bridge-url` flag [done on this branch], (2) `bridge.rs` + event tap + command receiver, (3) `invoke_handler` tap, (4) Node bridge + `replay-chat`, (5) other scenarios (sniffer, updater, K18). One PR at a time.
