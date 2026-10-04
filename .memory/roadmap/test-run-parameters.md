# Plan -- run the app for tests with parameters (written 2026-10-04, NOT started)

Kade (2026-10-04): "make a plan for app run with useful parameter for testing: APPDATA directory or
other things you need when you set test environments." This is the plan only: **nothing here is
built, and the decisions at the end are Kade's.** Order of work follows CLAUDE.md (plan first, test
first, one `claude/*` PR at a time).

## Why

The W1 notebooks (`test/w1-*`, see `sessions/2026-10-04-w1-notebooks-handoff.md`) drive the real exe on
Kade's PC. Four runs showed what is missing:

1. **A copy shares the installed app's data folder** (same identifier), so a test run reads and
   writes Kade's real `config.json`, model, chat logs. The notebook had to *edit his config*
   (`mark_setup_done`) and offer an undo -- fragile.
2. **A fresh copy shows the setup wizard, which never offers an app update**
   (`src/app/hydration.rs`: the update check runs only in the `init_done` branch). So the update test
   needs a set-up app, which means a config file or a completed wizard (model download).
3. **A tester cannot tell which copy is running** or what it resolved (the A1 run changed another
   folder than the one watched). Release builds are GUI-subsystem
   (`#![windows_subsystem = "windows"]`, `src-tauri/src/main.rs`) and `env_logger` writes to the
   console only, so a launched release exe tells a script nothing.
4. **Side effects a test does not want**: the sniffer (needs Administrator and the firewall rule),
   llama-server, the update check, popup pre-creation, the window-state file.

## What exists today (checked in the code, 2026-10-04)

- No command-line or environment parameters at all: `env::args` is read only by the
  `verify_update` example.
- The app's folders come from Tauri's resolver at **14 call sites**:
  `app_config_dir()` -- `config/app_config.rs:186` (config.json), `config/metadata.rs:30`;
  `app_data_dir()` -- `io/data_factory.rs:23,73`, `io/fs.rs:13,47`,
  `services/downloader/{gist.rs:128, model.rs:14,38, server.rs:16,32}`,
  `services/sniffer/raw_capture.rs:26`, `services/translator/server_manager.rs:22,103`.
  Plus two that are *not* ours: `tauri-plugin-window-state` keeps its file in `app_config_dir()`
  itself, and WebView2 keeps its profile under `%LOCALAPPDATA%\<identifier>\EBWebView`.
- URLs are constants in `services/downloader/gist.rs` (`METADATA_URL`, `FEED_URL`, `DICT_URL`) and
  `server.rs` (`AI_SERVER_ZIP_URL`). Downloads must be HTTPS (`resonance_core::download`).
- Windows identifier: `com.enjay.bpsr.resonance-stream` (`tauri.conf.json`); config is
  `%APPDATA%\com.enjay.bpsr.resonance-stream\config.json`.

**Believed, not verified:** overriding the `APPDATA` / `LOCALAPPDATA` *environment variables* does
**not** move the folders, because the `dirs` crate under Tauri asks Windows for the known folder
(`SHGetKnownFolderPath`), not the variable. A 5-minute probe settles it **before any code is
written** (Step 0). If it does redirect, P2/P3 shrink to nothing for the data dir.

## Step 0 -- probe (Kade, Windows, 5 minutes, no code)

In a PowerShell window:

```
$env:APPDATA = "C:\w1probe\roaming"; $env:LOCALAPPDATA = "C:\w1probe\local"
& "...\Resonance-Stream-v0.6.1.exe"        # let it open, close it
Get-ChildItem C:\w1probe -Recurse | Select FullName
Test-Path "$env:USERPROFILE\AppData\Roaming\com.enjay.bpsr.resonance-stream\config.json"
```

`C:\w1probe` filling up = the variables work (then the plan is mostly a documentation task).
Empty, with the real folder touched = a code-level override is needed (the rest of this plan).
Either answer goes into this file.

## The parameters (what a test run needs)

Command-line flags win over environment variables (`RESONANCE_TEST_<NAME>`), so a notebook
passes flags and `cargo tauri dev -- -- --data-dir ...` works for development.

| flag | does | why a test needs it |
|---|---|---|
| `--data-dir <dir>` | root of everything the app writes: `<dir>\config` (config.json, metadata), `<dir>\data` (model, server, chat logs, dictionary, raw capture), `<dir>\webview` (sets `WEBVIEW2_USER_DATA_FOLDER`) | isolation: never touch Kade's real data; one folder per run, deletable |
| `--fresh` | empty `--data-dir` first | a clean first-run, repeatable |
| `--assume-setup-done` | behave as `init_done: true` for this run, **without writing it** | update dialog and the main window without the wizard or a model download |
| `--no-capture` | no sniffer, no firewall check / wizard | run without Administrator or the game |
| `--no-translator` | do not start llama-server | no model needed, no port 8080 |
| `--no-update-check` | skip the start-up check | stable UI runs |
| `--feed-url <url>` / `--metadata-url <url>` | read the update feed / gist metadata from here | test "no update", "feed 404", "bad signature feed" against a local mock server |
| `--no-popups` | do not pre-create the popup windows | faster, quieter start |
| `--no-window-state` | do not restore or save window size / place | the plugin writes to `app_config_dir()` we cannot redirect |
| `--status-file <path>` | writes JSON at start and when ready (below) | a script waits for *ready* and knows *which exe* ran |
| `--log-file <path>` | also write the log there | a GUI-subsystem release exe has no console |
| `--print-env` | print the resolved settings as JSON and **exit** (no window) | prove a parameter took effect; the Step 0 probe in one line |
| `--replay-chat <file.jsonl>` | feed recorded chat lines in as if captured (timed) | UI and translator tests without the game (phase 2, optional) |

`--status-file` JSON (stable keys): `version`, `exe` (full path), `pid`, `ready` (bool),
`config_dir`, `data_dir`, `flags` (what was set), `update` (`none` / `available:<version>` /
`downloading` / `downloaded` / `error:<reason>`). The updater notebook then replaces its
"which copy is running" guess with a fact and can wait for `downloaded` instead of asking a person.

## Design (so it cannot leak into a release)

1. **Pure parsing in `crates/core`** (`test_env`): `parse(args, env) -> TestEnv` -- unknown flag =
   error, every flag tested on every OS. `resolve_dirs(default_config, default_data, &TestEnv) ->
   AppDirs`, same.
2. **One choke point in the app**: `AppDirs` in `AppState` (or a `OnceLock`), and the 14 call sites
   call it instead of `app.path()`. With no flags it returns exactly what Tauri returns today --
   **a refactor, no behaviour change** (CLAUDE.md), its own PR, `app-check` on Windows CI.
3. **Compile-time gate**: the flags are honoured only when `cfg(any(debug_assertions, feature =
   "test-env"))`. A normal release exe ignores them (and says so for `--print-env`). Release
   candidates are test builds: `release-candidate.yml` adds `--features test-env`; `release.yml`
   never does.
4. **Released exes cannot take parameters** (0.6.0, 0.6.1 have none). The updater test that starts
   from 0.6.0 keeps the config-file preparation it has now (`updater.mark_setup_done`); everything
   from the first version with `test-env` on can use the flags, and the notebooks move over.

## Work, in order (each its own `claude/*` branch and PR, test first)

| # | PR | gate | verified by |
|---|---|---|---|
| 0 | **probe** (above) | -- | Kade; answer recorded here |
| 1 | `core::test_env`: `parse`, `resolve_dirs` (+ tests) | core | `just core-check` |
| 2 | app: `AppDirs` choke point, 14 call sites, **no behaviour change** | app | CI Windows job; a run on Windows |
| 3 | app: gate + `--data-dir`, `--fresh`, `--print-env`, `--status-file`, `--log-file` | app | CI; `--print-env` from a notebook |
| 4 | app: `--assume-setup-done`, `--no-capture`, `--no-translator`, `--no-update-check`, `--no-popups`, `--no-window-state` | app | CI; a run |
| 5 | app: `--feed-url`, `--metadata-url` (HTTPS only; `http://127.0.0.1` only under the gate) | app | mock-server notebook |
| 6 | `release-candidate.yml`: build with `--features test-env` | CI | the next candidate's `--print-env` |
| 7 | notebooks (`test/w1-*`) use the flags; the config-editing step stays only for released exes | -- | a Windows run |
| 8 | `--replay-chat` (wanted, decision 4) | core + app | fixture replay |

Gates that cannot run in a Linux session (anything in `src-tauri/`) are named in the commit body
and left to the Windows CI job, as always.

## Decisions (Kade, 2026-10-04 -- answered, recommended options except the last)

1. **Gate**: debug builds + `test-env` feature only. A stable release ignores every flag.
2. **Release candidates get `test-env`** (PR 6); `release.yml` never does.
3. **Flags and env vars**: `RESONANCE_TEST_<FLAG>`, flag wins (built in `core::test_env`).
4. **`--replay-chat` is wanted now** (PR 8). The line format is still open -- proposed one JSON
   object per line, `{"delay_ms": 500, "text": "...", ...}`; confirm the fields before building.
5. **`--feed-url` / `--metadata-url`**: `https://`, or `http://` to 127.0.0.1 / localhost / [::1]
   only (`core::test_env::is_test_url_allowed`). PR 5 adds the matching exception to the
   downloader's `check_download_url`, behind the gate.

## Progress

- **PR 1 `claude/test-env-core` -- done**: `resonance_core::test_env` (`parse`, `TestEnv`,
  `resolve_dirs`, `AppDirs::reset` for `--fresh`, `is_test_url_allowed`; 22 tests). Safety rules
  pinned by tests: `--fresh` needs `--data-dir`; `reset` empties only `config/`, `data/`,
  `webview/` and refuses a root near the top of a drive or with `..`.
- **PR 2 `claude/test-env-appdirs` -- done, app gate left to Windows CI**: `src-tauri/src/app_dirs.rs`
  (`config(app)`, `data(app)`, both still exactly Tauri's `app_config_dir` / `app_data_dir`); the 14
  call sites use it; a test (`no_code_outside_this_module_asks_tauri_for_a_folder`) fails if any
  other file calls Tauri for a folder again. Cross-checked on Linux, not run (no GTK here).
- **PR 3 `claude/test-env-flags` -- done, app gate left to Windows CI**: Cargo feature `test-env`;
  `src-tauri/src/test_env.rs` (`GATE_OPEN = debug_assertions | feature`; `init()` first in `run()`);
  `--data-dir` (via `app_dirs`, plus `WEBVIEW2_USER_DATA_FOLDER`), `--fresh`, `--print-env`,
  `--status-file` (written at start, at ready, and on each update step: `check_all_updates`,
  `download_app_update`), `--log-file` (tee with the console, no colours). Core side:
  `StatusReport`, `UpdateState` (+ `after_check`), `TestEnv::restart_args`, `Tee`; 8 new tests.
  `restart_to_apply_update` passes the flags on (never `--fresh` / `--print-env`).
  **Known gap (PR 3):** the other flags parsed but did nothing until PR 4 / 5. A release exe with `test-env` is a GUI-subsystem exe, so
  `--print-env`'s stdout may not reach a console -- use `--status-file` with it (it writes the same
  JSON). `config_dir` / `data_dir` are `null` in `--print-env` without `--data-dir` (Tauri's
  defaults are not known before the app exists); the status file fills them in at ready.
- **PR 4 `claude/test-env-behaviour` -- done, app gate left to Windows CI**: the six switches act.
  `--assume-setup-done`: `init_done` is true for the run (`run()` reads it through
  `test_env::init_done_for_run`) and `apply_config` writes the *stored* value back, so the flag never
  reaches `config.json` (rules in `TestEnv::init_done_for_run` / `_for_disk`, tested). `--no-capture`:
  `start_sniffer_command`, `start_sniffer_worker` and `ensure_firewall_rule_command` do nothing (sniffer
  state Off). `--no-translator`: `start_translator_worker` starts no server; a pass-through thread
  archives each chat as translator-off does. `--no-update-check`: `check_all_updates` answers "nothing"
  without the network (the settings button too). `--no-popups`: no `prewarm_popups`. `--no-window-state`:
  the window-state plugin is not registered, and `show_popup` skips `restore_state` (it would panic
  without the plugin). `--feed-url` / `--metadata-url` still parse and do nothing (PR 5).
- **PR 5 `claude/test-env-feed-urls` -- done, app gate left to Windows CI**: `--feed-url` and
  `--metadata-url` are used by `check_all_updates` (`fetch_update_feed`, the gist metadata read). A
  test run that sets either may also download from `http://127.0.0.1` / `localhost` / `[::1]`:
  `download::check_download_url_allowing(url, allow_local_http)`, `update_feed::parse_feed_allowing`,
  `test_env::is_local_http_url` (core, tested; the plain `check_download_url` / `parse_feed` are
  unchanged and still HTTPS only). The signature check on a downloaded exe is not relaxed -- a mock
  feed must carry a valid signature, or test the "bad signature" path on purpose. **Not covered:**
  the dictionary (`DICT_URL`) and the llama-server zip (`AI_SERVER_ZIP_URL`) still come from their
  fixed URLs.
- **PR 6 `claude/test-env-rc-build` -- done, first real build is the next candidate**:
  `release-candidate.yml` builds with `--features test-env` and its Korean tester notes get a line 5
  about the test options. `rc-lib.test.sh` pins it (runs in CI): the candidate build has the feature,
  `release.yml` never mentions `test-env`, the notes line is there. Not run: an actual candidate
  build -- check the first one's exe with `--print-env --status-file <file>`.
- **Windows check by Kade (2026-10-05, local build `--features test-env`, PRs 3-5 code on `main`)**: built
  locally with `npx --yes @tauri-apps/cli@2 build --no-bundle --features test-env`; "works well": `--data-dir
  C:\w1\run1 ... --status-file C:\w1\run1.json` made `run1` and `run1.json` and left his real data alone.
  **Open question to Kade:** he pressed the in-app "open app data folder" and the screenshot shows Explorer on
  `C:\w1` (run1 selected) -- the code opens `data` = `C:\w1\run1\data`, so ask what the address bar said.
- **PR 7 `test/w1-updater-mock` -- done, never merged into `main`**: a NEW notebook
  `test-w1/notebooks/w1-updater-mock.ipynb` (the six existing notebooks are untouched). It serves a signed "new
  version" (the exe under test + appended bytes, signed with the backup key) from a mock server on 127.0.0.1 and
  runs a fresh copy per check with the flags; outcomes come from the status file and the copy's folder. Checks:
  M0 flags understood, M1 signing, M8-M12 feed cases (no clicks), M3 good update, M4 tampered download refused,
  M5-M7 bad signature / cut / silent download -> `update: error`, M-iso real config untouched. Helpers
  `w1/mockfeed.py` (27 tests), stand-in `tests/fake_app.py`, dry run of every cell (4 tests); 138 pass from a fresh
  clone. **Not run on Windows or with the real exe.** The old notebooks keep their config-file step (released exes).
- Path call sites for PR 2 were **14**, not 12: `app_config_dir()` x2 (`config/app_config.rs:187`,
  `config/metadata.rs:31`) and `app_data_dir()` x12 (`io/data_factory.rs`, `io/fs.rs`,
  `downloader/{gist,model,server}.rs`, `sniffer/raw_capture.rs`, `translator/server_manager.rs`).
- Step 0 (Kade's probe) is still not answered; PR 2 can go ahead without it.

## Not in this plan

Renaming or moving `config.json`; changing the real data layout; any change to what a normal
(unflagged) run does; the product question *"a brand-new user is never offered an app update until
setup is finished"* (noted in the handoff -- decide separately).
