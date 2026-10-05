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
| `notebooks/updater.ipynb` | K2 with the released 0.6.0 exe: A1, A2, A3 (a note), A4, A5 |
| `notebooks/updater-mock.ipynb` | K2 against a mock feed, in a data folder of its own (needs a `test-env` exe) |
| `notebooks/feed.ipynb` | K3: the release feed check |
| `notebooks/firewall.ipynb` | K6: the firewall rule per exe |
| `notebooks/interface.ipynb` | K4: the route-based interface pick |
| `notebooks/replay-chat.ipynb` | the `--replay-chat` test flag: start a copy, read its log, ask what the window showed (needs a `test-env` exe) |
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
mock-feed notebook's dry run drives `tests/fake_app.py`, a stand-in that follows the app's flags
and status file.

Run `pytest` from this folder (`runbook/`); the full suite takes about three minutes.
