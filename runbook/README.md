# The runbook: manual checks on Windows

One Jupyter notebook per job, run by hand on a Windows PC. Each does what a script can do
(download, hash, `netsh`, `route print`, signing, polling Actions, starting the app with its test
flags) and **asks** for the rest (clicking in the app, the game, cutting the network). Every
check ends as `pass` / `fail` / `skip` with evidence; the last cell prints one report block --
paste it back to Claude.

This folder lives on `main`, next to the code it tests. (Until 2026-10-05 each notebook had its
own `test/w1-*` branch; those branches are an archive now -- fix a notebook here, in a normal
`claude/*` PR.) The notebooks were written for the roadmap's Windows session W1 (K2, K3, K4, K6, K7,
K25); later sessions add notebooks here, so the folder is not named after one session.

| notebook | job |
|---|---|
| `notebooks/updater-mock.ipynb` | K2 against a mock feed, in a data folder of its own, **driven over the bridge, no clicks** (needs a `test-env` exe and Node). The steps are Python: `runbook/pipelines/updater_mock.py`; without Jupyter: `python -m runbook.run updater-mock --key <backup key file>` (the exe defaults to `target\release\resonance-stream.exe` of this checkout; `--exe` or `RUNBOOK_TEXT_LOCAL_EXE` overrides it). (The notebook for the released 0.6.0 exe, `updater.ipynb`, was retired 2026-10-05: that exe cannot use the bridge.) |
| `notebooks/window-restore.ipynb` | K18: closing with Settings open must not save the enlarged window. Over the bridge, no clicks: `grow-window`, leave by `quit` (the tray's Quit) and by `close-window` (the X), start again on the same data folder, `snapshot` must match the size before the grow. `python -m runbook.run window-restore` (needs a `test-env` exe with #159 or newer) |
| `notebooks/capture-spike.ipynb` | the capture end to end with no game: this PC plays the game server (TCP port 5003, real chat frames from `resonance_core::capture::synth`) and the app's sniffer must publish them as `packet-event`s -- tried on the LAN address and on 127.0.0.1 (does Windows show a PC's own traffic to a raw socket?). `python -m runbook.run capture-spike [--add-firewall-rule]` (a `test-env` exe, Node and Rust; the firewall rule is only made when asked) |
| `notebooks/feed.ipynb` | K3: the release feed check |
| `notebooks/firewall.ipynb` | K6: the firewall rule per exe |
| `notebooks/interface.ipynb` | K4: the route-based interface pick, read from the bridge (no pasting): the app starts with the sniffer on and says which adapter it took; the pipeline looks at this machine (two adapters / full-tunnel VPN / offline) and runs the check that fits, the others are `skip`. Switching the VPN or the network is physical: do it, run again. `python -m runbook.run interface`. "Chat is captured" needs game traffic (`skip`) |
| `notebooks/replay-chat.ipynb` | the `--replay-chat` test flag: start a copy, read its log and what it published on the bridge, ask what the window showed (needs a `test-env` exe; Node for the bridge rows) |
| `notebooks/checklists.ipynb` | K7, Esc, K25 (read from `.memory/active-issues/unverified-on-windows.md` at run time) |

Code: the Python package `runbook/runbook/` -- `common.py` (result recorder, prompts, command capture) and
one `<job>.py` of helpers per notebook -- and `tools/dryrun.py` (headless runs). Tests: `tests/test_<job>.py`
and `tests/test_notebook_<job>.py` (the dry run); `tests/fixtures/` holds the recorded command output the
dry runs use.

## The exe for `updater-mock`

It needs an exe built with the app's test flags (`--features test-env`; the released 0.6.0 / 0.6.1
ignore them). From a `main` checkout, in PowerShell: `npm ci`, `rustup target add
wasm32-unknown-unknown`, `cargo install trunk --locked` (once), then
`npx --yes @tauri-apps/cli@2 build --no-bundle --features test-env` -- the exe is
`target\release\resonance-stream.exe` (or take a release candidate built after PR #131). The
notebook asks for its path and checks that it understands the flags.

## The bridge (`bridge/`, Node 20+)

A test exe started with `--bridge-url mqtt://127.0.0.1:PORT` publishes what it does to a local MQTT broker (every event it
sends the window, every command the window sends it) and obeys commands published there (`ping`, `quit`, `replay-chat`; the
topics are in `crates/core/src/bridge.rs`). `bridge/` is the other end: `bridge.mjs` (broker + recorder + `expect` /
`expectSequence` / `send`), `scenarios.mjs` (what a scenario must have published) and `cli.mjs` (for the notebooks, through
`runbook/bridge.py`). `replay-chat.ipynb` uses it for its B0-B5 rows. Once, in `runbook/bridge/`: `npm ci`
(the notebook does it for you). **No notebook, no prompts:** `node runbook/bridge/cli.mjs run replay-chat --exe C:\path\to\resonance-stream.exe` starts the broker and the exe, sends `ping`, `replay-chat` and `quit` over the bridge, checks what the app published (A1-A3, B0-B5, Q1) and prints the report as JSON (progress on stderr; exit 0 = all passed). The window still opens, and the manual rows (what the window drew) stay in the notebook. Tests: `cd bridge && npm test` (about 3 s; the app is stood in for by a small MQTT client, so it
proves the bridge, not the app).

## Run (Windows)

```
git checkout main && git pull
cd runbook
py -m venv .venv && .venv\Scripts\activate
pip install notebook
jupyter lab                                             # from an ADMINISTRATOR terminal
```

Open a notebook from `notebooks/` and run the cells top to bottom, one at a time. A prompt cell
waits for `pass`, `fail`, or `skip`, optionally with a note (`fail: bar stays at 0%`). A typo
counts as `skip`, never `pass`. The first cell of a Windows job checks that Jupyter is elevated.

Results are also saved as JSON under `runs/` (ignored by git). **Saved outputs make a notebook
"modified" and block `git pull`:** before pulling, run `git checkout -- runbook/notebooks`
(this throws away the outputs of your run -- paste the report to Claude first). Commit notebooks
**without outputs** (Kernel > Restart & Clear Outputs, or
`jupyter nbconvert --clear-output --inplace`).

## Without Windows (what Claude can check)

```
pip install nbformat nbclient ipykernel pytest
python -m pytest -q                                                    # helpers + dry runs
python tools/dryrun.py notebooks/<job>.ipynb "pass,pass,fail: why"   # headless run
```

`RUNBOOK_DRYRUN=1` (set by `dryrun.py`) swaps every command for a recorded fixture in
`tests/fixtures/`, and the answers feed the prompts. That proves the cells run and the checks
read what they should. It does **not** prove anything about Windows, the game, or the app. The
mock-feed pipeline's dry run drives `tests/fake_app.py`, a stand-in that follows the app's flags
and status file and speaks to the bridge's MQTT broker (a minimal client of its own).

Run `pytest` from this folder (`runbook/`); the full suite takes about three minutes.
