# Handoff: Kade's Windows runs, with the runbook on main (2026-10-05)

For a **new session** with no context. Read `CLAUDE.md` first -- especially *The runbook is docs* and
*Version Control* -- then `MEMORY.md`. Kade runs the checks on his Windows PC and pastes the report
blocks back; this session turns what they show into fixes.

## Where things stand

- **The test-run parameters are built and merged** (PRs #126-#133, plan and decisions in
  `roadmap/test-run-parameters.md`): `--data-dir`, `--fresh`, `--print-env`, `--status-file`, `--log-file`,
  `--assume-setup-done`, `--no-capture`, `--no-translator`, `--no-update-check`, `--no-popups`,
  `--no-window-state`, `--feed-url`, `--metadata-url`, `--replay-chat`. Honoured only in debug builds and
  builds with `--features test-env` (release candidates have it, stable releases never do).
- **Verified on Windows so far:** only `--data-dir` + `--status-file` (Kade, local build, 2026-10-05) and
  the old updater notebook's A3, A4, A5. **Not yet run on Windows:** the other flags, `--replay-chat`, the
  mock-feed notebook, and every other W1 notebook except those parts.
- **The runbook is on `main`** in `test-w1/` (6 notebooks, see `test-w1/README.md`). It is docs: no CI job,
  no gate. The old `test/w1-*` branches are an archive; do not update them.
- Every step of `w1-updater` and `w1-updater-mock` closes its app by itself (`updater.step`: try / catch /
  finally), so a step never leaves a window for the next one.

## What Kade will run (in this order, he decides)

1. Build a local test-env exe from `main` (README of `test-w1/`: `npx --yes @tauri-apps/cli@2 build --no-bundle
   --features test-env`).
2. `notebooks/w1-updater-mock.ipynb` with that exe (needs the backup key + password; Administrator Jupyter).
   Checks M0-M12, M-iso. This should show **where the 0.6.0 update files go**, which the old run could not.
3. `--replay-chat crates\core\testdata\replay-sample.jsonl` with `--data-dir ... --fresh --assume-setup-done
   --no-capture --no-translator --no-update-check`: eight Japanese lines appear over ~10 s across the WORLD /
   GUILD / PARTY / LOCAL / BEGINNER tabs; `<sprite=3>` shows `[이모지]`, `emojiPic=9` shows `[스티커]`; the
   system log says "Replaying 8 chat lines" then "Replay finished".
4. The other notebooks (`w1-feed` K3 + K3b, `w1-firewall` K6, `w1-interface` K4, `w1-checklists` K7 / Esc / K25),
   none of which has had a real run.

## What to do with a report

- Tick the matching items in `roadmap/roadmap-since-0928.md`, delete the matching bullet in
  `active-issues/unverified-on-windows.md`, write failures up in a new `sessions/` note.
- A failure in **app logic**: test first, on its own `claude/*` branch, one PR at a time (CLAUDE.md).
- A failure in the **notebook** (a wrong check, a bad path): fix it in `test-w1/` -- docs, no test-first, no CI
  job. Run `cd test-w1 && python -m pytest -q` if you touched helpers (about 3 minutes; a courtesy).
- The open finding: the old `w1-updater` run (2026-10-05, released 0.6.0 copies) failed A1 (exe unchanged, no
  `.old`), A1's `.old` row and A2-flip (no `update_temp.exe`) while Kade answered `pass` in the UI, and
  A2-which saw two copies running (the A1 app was left open). Not diagnosed. Do not guess: wait for the
  mock-feed report (its status file names each run's exe path and pid).

## Not done

- `test-w1/open.cmd` (optional, see MEMORY.md). `test/w1-*` branches are for Kade to delete if he wants.
- Two pushed commit messages on the archived branches state wrong test counts (`7722bcb` says 119, it is
  120; `d70b45e` says "9 new tests", it is 6). Not rewritten.
- The product question from the earlier handoff: a brand-new user is never offered an app update until setup
  is finished (`src/app/hydration.rs`). Put it to Kade; undecided.

## Habits that paid off, and mistakes to avoid

- Stage named files, run `git show --stat HEAD` before pushing (an earlier `git add -A` swept in `__pycache__`).
- Count tests from the real run, not from memory (two commit messages are wrong).
- `pkill -f <name>` kills your own shell when the pattern is in its command line.
- This Linux session cannot build `src-tauri/`; cross-check with `cargo check -p resonance-stream --target
  x86_64-pc-windows-gnu --tests` (and `--release`, `--release --features test-env`) and say the app gate is left to
  Windows CI. Never report it as passed.
- A step that stubs out code to watch a test fail may be refused by the harness; a missing method / item
  (compile error) is an equally honest "red".

## Prompt for the new session

> Read `CLAUDE.md`, `MEMORY.md` and `.memory/sessions/2026-10-05-runbooks-on-main-handoff.md`. I am Kade. I will
> paste report blocks from the notebooks in `test-w1/` and tell you what the app showed. Turn failures into
> test-first fixes (app logic) or direct notebook fixes (the runbook is docs); one PR at a time.
