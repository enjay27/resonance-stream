# Handoff: the W1 verification notebooks, and where the run stands (2026-10-04)

For a **new session** with no context. Kade runs the checks on his Windows PC; this session's job
is to keep the notebooks right and to turn what they find into test-first fixes. Read `CLAUDE.md`
first (plan first, TDD, one `claude/*` PR at a time, `test/*` never merged into `main`).

## Where things stand

- **v0.6.1 is released** (2026-10-03). On `main` since: K26 favorites tabs by stable id (#120),
  Esc closes the popup windows (#119), the `test/*` guard (#121), memory notes (#124). Next release
  is 0.6.2 (K5 + whatever W1 finds). Roadmap: `roadmap/roadmap-since-0928.md`.
- **The W1 session** (K2, K3, K4, K6, K7, K25 -- `roadmap-since-0928.md`) is run from Jupyter
  notebooks, one `test/w1-*` branch per job. They do what a script can and prompt for the rest.
- **Verified on Kade's PC so far (his real runs):**
  - the notebook machinery on Windows 11 as Administrator: v0.6.0 download + SHA-256 against the
    published sums (K2-0);
  - **A5 (the offline backup key): passed.** `tauri signer sign` with the key's contents in
    `TAURI_SIGNING_PRIVATE_KEY` (as `release.yml` does) signed a file; key id
    `0099CF719FD83012` (the backup, not the primary `9005FB94...`); the app's own
    `verify_update` accepts it for 0.6.0 and refuses it for 0.6.9. The roadmap's A5 can be ticked.
- **Not yet validly run:** A1-A4 (below), K3 (feed workflow), K3b (key copies), K4, K6, K7, K25.

## The branches (all pushed; none may ever be merged into `main`)

| branch | job | notebook | pytest (fresh clone, last run) |
|---|---|---|---|
| `test/w1-base` | shared helpers + README | -- | 45 |
| `test/w1-updater` | K2: A1, A2, A3, A4, A5 | `test-w1/notebooks/w1-updater.ipynb` | 107 |
| `test/w1-feed` | K3 (+ K3b prompt) | `w1-feed.ipynb` | 72 |
| `test/w1-firewall` | K6 | `w1-firewall.ipynb` | 62 |
| `test/w1-interface` | K4 | `w1-interface.ipynb` | 68 |
| `test/w1-checklists` | K7 + Esc + K25 (read from `.memory/active-issues/unverified-on-windows.md` at run time) | `w1-checklists.ipynb` | 74 |

Job branches are cut from `test/w1-base`; after changing the base, `git merge test/w1-base` into each
(they were merged and re-verified from fresh clones at the end of this session). `main` is merged
*into* `test/w1-base`, never the reverse. Guard: `.github/workflows/test-branch-guard.yml` (seen
failing on a throw-away `test/*` PR). Stale branch to delete by hand: `claude/w1-notebooks-memory`
(closed PR #123; the git proxy refused the delete).

How a run works: `test-w1/README.md`. In short: `git checkout test/w1-<job>`, `cd test-w1`, a
venv with `pip install notebook`, `jupyter lab` from an **Administrator** terminal, run cells top
to bottom, answer prompts `pass` / `fail: what you saw` / `skip: why` (an empty or unclear answer
asks again), paste the final report block back. **Saved outputs make the notebook "modified" and
block `git pull`:** `git checkout -- test-w1/notebooks/<job>.ipynb` first.

Without Windows: `pip install nbformat nbclient ipykernel pytest`, `python -m pytest -q` in
`test-w1`, `python tools/dryrun.py notebooks/<job>.ipynb "answers,..."` (`W1_DRYRUN=1`: commands
become recorded fixtures in `tests/fixtures/`). That proves cells run; it proves nothing about Windows.

## The open finding (why A1-A4 are not done)

Four runs of the updater notebook went: everything skipped (Enter counted as skip) -> A1/A2 `pass`
by Kade but the watched folder unchanged -> all skipped again. Cause, from the code and Kade's
answer ("a new fresh app without the installed model and config"):

- The update dialog only appears in an app that finished first-run setup
  (`src/app/hydration.rs`: `check_all_updates` runs in the `init_done` branch; the wizard calls it
  only for model metadata and never offers an app update). **A fresh copy cannot be updated.**
- Kade's earlier A3 / A4 `pass` answers were on the wizard screen and **do not count**.
- The app swaps files in the folder of the exe that runs (`app_updater.rs`:
  `current_exe().parent()`: `update_temp.exe`, `<exe>.old`).
- The notebook now: starts each copy itself (`updater.launch`), has a **prepare step** that, only on
  a typed `yes`, sets `init_done: true` in `%APPDATA%\com.enjay.bpsr.resonance-stream\config.json`
  (other keys kept, original backed up, undo offered at the end), checks the running process is
  the copy (`A2-which`), and prints every copy folder's state when A1/A2 fail (`show_copies`).
- **Unverified:** that `{"init_done": true}` alone gets the app past the wizard (core tests parse
  it; nobody ran the app), and the config folder path on Kade's machine.

**Kade's next action:** pull `test/w1-updater` (see the `checkout --` note), restart the kernel, run
from the top, type `yes` at the prepare step, do A1, A2, A3, A4 with real answers; A5 may be skipped
(done). If the dialog still does not appear, complete the setup wizard once by hand instead and tell
the session what the app showed.

## What the next session should do

1. Wait for Kade's report; tick K2 pieces in `roadmap-since-0928.md` as they pass, delete the
   matching bullet in `.memory/active-issues/unverified-on-windows.md`, write failures up in a new
   `sessions/` note and fix each **test first on its own `claude/*` branch**.
2. **Plan to build:** `roadmap/test-run-parameters.md` -- a data-dir override and friends so tests stop
   touching Kade's real data. **Nothing is built**; Step 0 is a 5-minute probe for Kade (does
   overriding `APPDATA` redirect?), and five decisions are his. Do not start P1+ before he answers.
3. Other W1 jobs still unrun: K3 (+ K3b: copies of both signing keys), K6, K4, K7, K25 -- each notebook
   is ready; none has had a real run except K2-0/A5.
4. Product question to put to Kade (not decided): a brand-new user is never offered an app update until
   setup is finished.
5. Housekeeping noted, not done: make `no test/* branch into main` a **required** check in `main`'s
   branch protection (only that stops a human merge; a PR can edit its own copy of the workflow);
   delete `claude/w1-notebooks-memory`; `MEMORY.md` is far over its ~40-line rule (K22).

## Mistakes made this session (so they are not repeated)

- `git add -A` on a branch cut from `main` swept 20 `__pycache__` files in (`main` has no
  `.gitignore` for `test-w1/`). **Stage named files, run `git show --stat HEAD` before pushing.**
  PR #123 was closed unmerged; #124 replaced it.
- `*.exe` is gitignored, so fake exe fixtures were never committed (fresh-clone test caught it);
  they are `.bin`. **Verify each branch from a fresh clone** (`git clone --branch ...`), not the work tree.
- The dry-run runner started the kernel in the wrong folder and so hid that Jupyter starts a notebook
  in its own folder (every notebook failed at cell 1 on a real PC); then `dryrun.execute` left
  `W1_DRYRUN` set and polluted later tests. Both fixed and tested.
- Test counts in two commit messages are wrong (they say 40 and 83; the real numbers were 43 and
  59). Pushed history is not rewritten.
- A handoff note's command (`TAURI_SIGNING_PRIVATE_KEY_PATH`) that nobody had run was copied into the
  notebook and failed on the first real run. Commands in notes are claims until run.

## Prompt for the new session

> Read `.memory/sessions/2026-10-04-w1-notebooks-handoff.md`, `CLAUDE.md` and
> `.memory/roadmap/test-run-parameters.md`. I am Kade. I will paste the W1 report blocks from the
> notebooks on `test/w1-*`. Plan first; tick or fix what the reports show, test first, one PR at a
> time; never merge a `test/*` branch into `main`. Do not build the test-run parameters before I
> answer the five decisions in that plan.
