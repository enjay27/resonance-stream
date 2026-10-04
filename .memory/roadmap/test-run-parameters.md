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
- The app's folders come from Tauri's resolver at **12 call sites**:
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
2. **One choke point in the app**: `AppDirs` in `AppState` (or a `OnceLock`), and the 12 call sites
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
| 2 | app: `AppDirs` choke point, 12 call sites, **no behaviour change** | app | CI Windows job; a run on Windows |
| 3 | app: gate + `--data-dir`, `--fresh`, `--print-env`, `--status-file`, `--log-file` | app | CI; `--print-env` from a notebook |
| 4 | app: `--assume-setup-done`, `--no-capture`, `--no-translator`, `--no-update-check`, `--no-popups`, `--no-window-state` | app | CI; a run |
| 5 | app: `--feed-url`, `--metadata-url` (HTTPS only; `http://127.0.0.1` only under the gate) | app | mock-server notebook |
| 6 | `release-candidate.yml`: build with `--features test-env` | CI | the next candidate's `--print-env` |
| 7 | notebooks (`test/w1-*`) use the flags; the config-editing step stays only for released exes | -- | a Windows run |
| 8 | optional: `--replay-chat` | core + app | fixture replay |

Gates that cannot run in a Linux session (anything in `src-tauri/`) are named in the commit body
and left to the Windows CI job, as always.

## Decisions for Kade

1. **Gate**: debug builds + `test-env` feature only (recommended), or always on? (Always-on lets a
   local user redirect a *released* app's data folder and update feed -- the update still must be
   signed by a built-in key, but there is no reason to ship the lever.)
2. **Release candidates get `test-env`** (recommended) -- they are already unsigned test builds.
3. **Flags and env vars** (recommended), or flags only?
4. Is **`--replay-chat`** wanted now, or later? (It is the only part that is real design work.)
5. **`--feed-url` with a local HTTP mock** (needs the localhost exception in the downloader), or
   only HTTPS URLs? Without it, "bad feed" tests need a real HTTPS host.

## Not in this plan

Renaming or moving `config.json`; changing the real data layout; any change to what a normal
(unflagged) run does; the product question *"a brand-new user is never offered an app update until
setup is finished"* (noted in the handoff -- decide separately).
