# Changelog: the automation session, PRs #167-#181 (2026-10-05 night to 2026-10-06)

What `.memory/sessions/2026-10-06-automation-plan.md` asked for, as built. Fifteen PRs, all merged to `main` by the
auto-merge workflow, one at a time. Kade's decisions that shaped it: "firewall: stay manual" (K6 stays a person's job; a
rule is made only with `--add-firewall-rule`), "stand-in llama server: just create a mock server, which responds exactly
like llama-server -- the real one loads the GPU", "Window runs: okay", and "without UI test automation" (plan item 10 is
not part of this).

## In one paragraph

A hosted Windows runner now starts the **real** app and drives it over the test bridge on every PR that touches the
bridge, the runbook helpers, the test flags, `window.rs` or the translator. Eight scenarios run there (replay-chat,
window-restore, capture-spike, chat-rules, persistence, download-integrity, translator-stub, popups). They found **two real
app bugs** (a blocked sender's old chat came back after a restart, #177; the bridge missed the translator's first states,
#181) and answered the open capture question (the sniffer does see this PC's own traffic). Beside the bridge: the bridge
and `rumqttc` are out of stable exes, property tests, config/wire-format parity tests and a weekly `cargo audit`.

## Changes, in merge order

### App / core / test infrastructure

| PR | what changed | why it matters |
|---|---|---|
| #167 | The bridge and `rumqttc` sit behind the `test-env` feature (`src-tauri/src/bridge/{mod,live}.rs`, no-op stubs when off, two source-scan tests). | A stable exe contains no test bridge and no MQTT client. |
| #169 | `crates/types/testdata/app_config_full.json` + `tests/wire_format.rs`; round-trip tests in `src/ui_types.rs` and `src-tauri/.../app_config.rs`. | A field added to one `AppConfig` only, or a renamed camelCase wire field, now fails a test. |
| #170 | `crates/core/tests/properties.rs` (proptest): stream assembler, parsers, readers, text. | Random input cannot panic the decoder or break its invariants. |
| #172 | `.github/workflows/audit.yml` (`cargo audit` on Cargo changes, Mondays, on demand; not in `ci.yml`). | Vulnerable dependencies are reported. **It reports reqwest 0.11 findings; nobody has decided the 0.12 upgrade** (K24). |
| #173 | `capture/synth.rs` (`live_chat_frame`, the inverse of the parser, round-tripped through `ChatPipeline`); examples `synth_frames`. | Real chat frames can be built without the game. |
| #174 | Commands `block-user`, `unblock-user`, `get-chat-history`, `clear-history`. | Chat rules are reachable from a test. |
| #176 | Command `download-model` + the `download-result` event. | The model download's end is observable. |
| #177 | **Bug fix.** `history::apply_block_list` (test first, red) called right after `load_recent` in `lib.rs`. | A sender blocked after their lines were archived came back **unflagged** after a restart (and an unblocked one stayed hidden). Found by `persistence` on the real app: `CP-block-reload`. |
| #178 | Flag `--llama-url` (core `test_env`: loopback http, a port, no path); `ServerGuard::external` (no child process); command `start-translator`; example `crates/llama/examples/translate_once.rs`. | The translator worker, health wait, three-failures restart and catch-up run unchanged against a stand-in server. Nothing changes without the flag. |
| #180 | Commands `open-popup`, `hide-popup`, `place-popup`, `pin-main`, `snapshot-popups`; helpers in `window.rs`. | Popup windows can be tried with real windows and no clicks (tray left out). |
| #181 | **Bug fix.** `bridge::start` now runs right after `AppState` is managed; `handle()` waits (60 s at most) for `test_env::is_ready()`. | With translation on, the translator started before the bridge listened, so its `Starting` / `Loading Model` events and the first system messages were lost. Found by `translator-stub`: `TS-ready` saw only `['Active']`. |

### Runbook (docs, no gate) and CI

| PR | what changed |
|---|---|
| #168 | `.github/workflows/bridge-smoke.yml` (windows-latest, elevated, interactive session, WebView2): builds the rc exe, runs the bridge unit tests and the pipelines. |
| #171 | Bridge Node tests: `fileURLToPath` instead of `new URL(..).pathname` (6 tests cancelled and 3 failed on Windows). Found by the first run of #168. |
| #173 | Pipeline `capture-spike` (this PC plays the game server on port 5003; the sniffer listens on the LAN address and on 127.0.0.1). `--add-firewall-rule` is the only way a rule is made. |
| #174 | Pipeline `chat-rules`: both dedupe layers, chat log equals what was published, block / unblock / clear. |
| #175 | Pipeline `persistence`: chat log, block list, pids across a restart on one data folder. No new app command. |
| #176 | Pipeline `download-integrity` (`MockServer` serves `/model.gguf`: ok, cut, 404): wrong SHA-256, cut-off, 404, no hash, not https. |
| #178 | `runbook/llama_stub.py` (stand-in llama-server: `/health` 503 "Loading model" then 200 starting at the first request; `/completion` in llama.cpp's shape; modes ok / error / empty / slot / close) and pipeline `translator-stub`. The K8 `<bos>` count is reported, not judged. |
| #179 | `.memory/active-issues/checklist-triage.md`: every bullet of `unverified-on-windows.md` sorted as covered / small / command / preview / human. |
| #180 | Pipeline `popups`. |

Every pipeline has a notebook, a README row, a pytest module and a stand-in-app model with **one bug switch per rule**
(each broken rule is caught by its own row). Whole runbook suite at the end: 322 pass.

## What ran on the real app (hosted Windows runner, one exe per run)

| pipeline | result | note |
|---|---|---|
| bridge unit tests | 28 / 28 | |
| replay-chat | 10 / 10 | |
| window-restore | 9 / 9 | K18 and the #163 fix hold on a real window |
| capture-spike | 5 / 5 | **The sniffer sees this PC's own traffic**, on the LAN address and on 127.0.0.1. Unlocks the "chat is captured" rows. |
| chat-rules | 7 / 7 | |
| persistence | 7 / 7 after #177 (6 / 7 before) | |
| download-integrity | 8 / 8 | the real refusal texts were seen |
| translator-stub | 1 / 3 rows judged, then crashed (run of #181) | `TS-ready` **passes** after #181: `Starting -> Loading Model -> Active`, 3 x 503 then up (it failed on #178's run). The next row, `TS-error`, is a runbook crash: `UnicodeDecodeError: 'charmap' codec can't decode byte 0x81` -- almost certainly `runbook/runbook/bridge.py:47` (`Popen(..., text=True)` with no `encoding`, so the broker's Japanese text is read as cp1252 on Windows). So the translate / prompt / restart / catch-up rows have **not** run on the real app yet. Not fixed here. |
| popups | 7 / 8 (the same in the runs of #180 and #181) | Prewarm, open once, X hides, pin, main X ends the app all pass. **`PP-place` fails: a popup left at 520x660 at (140,110) comes back 520x**690** after a restart** (x, y, width right; height +30 px). Probably the same outer-vs-inner size drift K18 had, on the popup path -- a real app finding, not fixed here. |

Everything except replay-chat and window-restore still runs with `continue-on-error`: the job is green even when those
rows fail, so **a green check on a smoke run does not mean these passed -- read the step list** (this masked the
`TS-ready` failure on #178).

## Found, not fixed (first jobs of the next session)

1. **`PP-place`: a restored popup is 30 px taller than it was left** (real app, reproduced twice). Look at `window.rs`
   `show_popup` / `apply_rect` against `inner_size_for` (#163 fixed the same drift for the main window).
2. **`translator-stub` crashes on Windows** after `TS-ready`: give the `Popen` calls in `runbook/runbook/bridge.py` (lines
   47 and 113) `encoding="utf-8", errors="replace"`. A runbook fix, no gate; then read the remaining
   `TS-*` rows -- they are the first look at the real translator path.

## Verified vs not

- Verified here on Linux every time: core + llama + types tests, `cargo fmt`, the runbook suite, the Windows
  cross-check (`--tests` with and without `test-env`, `--release`), compile only.
- **The app gate (`cargo test -p resonance-stream`) never ran in these sessions** (no Windows toolchain); CI's
  `windows-latest` job ran it on every PR and was green at merge. Tauri glue (start-up order, `ServerGuard::external`,
  command handlers) is covered only by the smoke runs above.
- No UI change, so no `ui-preview` screenshot and no manual `cargo tauri dev` run were needed.

## What this session did not do

See [`../roadmap/test-automation.md`](../roadmap/test-automation.md) for the open list and the next steps.
