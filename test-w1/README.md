# W1 verification notebooks (`test/w1-*` branches -- never merged into `main`)

One Jupyter notebook per W1 job. Each does what a script can do (download, hash, `netsh`,
`route print`, signing, polling Actions) and **asks** for the rest (clicking in the app, the
game, cutting the network). Every check ends as `pass` / `fail` / `skip` with evidence; the last
cell prints one report block -- paste it back to Claude.

| branch | job | notebook |
|---|---|---|
| `test/w1-updater` | K2: A1, A2, A4 (A3 note), A5 | `notebooks/w1-updater.ipynb` |
| `test/w1-feed` | K3: release feed check | `notebooks/w1-feed.ipynb` |
| `test/w1-firewall` | K6: firewall rule per exe | `notebooks/w1-firewall.ipynb` |
| `test/w1-interface` | K4: route-based interface pick | `notebooks/w1-interface.ipynb` |
| `test/w1-checklists` | K7, Esc, K25 | `notebooks/w1-checklists.ipynb` |

`test/w1-base` is what they share (`w1/common.py`, `tools/dryrun.py`); each job branch is cut
from it.

## Run (Windows)

```
git fetch origin && git checkout test/w1-updater        # the job you run
cd test-w1
py -m venv .venv && .venv\Scripts\activate
pip install notebook
jupyter lab                                             # from an ADMINISTRATOR terminal
```

Open the notebook and run the cells top to bottom, one at a time. A prompt cell waits for
`pass`, `fail`, or `skip`, optionally with a note (`fail: bar stays at 0%`). A typo counts as
`skip`, never `pass`. The first cell of a Windows job checks that Jupyter is elevated.

Results are also saved as JSON under `runs/` (ignored by git). Commit notebooks **without
outputs** (Kernel > Restart & Clear Outputs, or `jupyter nbconvert --clear-output --inplace`).

## Without Windows (what Claude can check)

```
pip install nbformat nbclient ipykernel pytest
python -m pytest -q                                                    # helpers
python tools/dryrun.py notebooks/<job>.ipynb "pass,pass,fail: why"   # headless run
```

`W1_DRYRUN=1` (set by `dryrun.py`) swaps every command for a recorded fixture in
`tests/fixtures/`, and the answers feed the prompts. That proves the cells run and the checks
read what they should. It does **not** prove anything about Windows, the game, or the app.
