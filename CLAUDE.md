# Agent Operating Rules — resonance-stream

Resonance Stream is a Windows desktop app: it sniffs Blue Protocol: Star Resonance
chat packets (raw socket, no client hooking), translates Japanese chat to Korean
through a local llama.cpp server, and shows it in a Tauri overlay.

It is one Cargo workspace in five crates, three parts, plus the runbook (`runbook/`, treated as
docs). **Which part you touch decides which gate applies.** That is the most important thing
on this page.

| tree | part | builds on | gate |
|---|---|---|---|
| `crates/core/` `crates/llama/` `crates/types/` | **core** — packet → chat pipeline, protocol decoding, translation text processing; llama-server HTTP client; DTOs shared by app and ui | any OS | `just core-check` |
| `src/` | **ui** — Leptos 0.8 CSR frontend (wasm); pure modules unit-tested on the host | any OS | `just ui-check` |
| `src-tauri/` | **app** — Tauri 2 backend: sockets, translator server, downloader, windows, tray | **Windows only** | `just app-check` (Windows) · `just app-cross-check` (Linux, compile only) |
| `runbook/` | **runbook** — Jupyter notebooks + Python helpers that Kade runs by hand on Windows (`runbook/README.md`) | any OS (dry runs) | **none — treated as docs** (see *The runbook is docs*) |

`just check` runs `fmt-check`, then every gate the current OS can run (`pip install
rust-just` or `cargo install just`). On Linux the app gate is a **compile-only**
cross-check against `x86_64-pc-windows-gnu` (needs `rustup target add
x86_64-pc-windows-gnu` and `apt install gcc-mingw-w64-x86-64`); it cannot link, so
the app's own tests run only on Windows. CI (`.github/workflows/ci.yml`) runs core +
ui on Linux and the full app gate on `windows-latest`, on every push and PR.

**New pure logic goes in `crates/core`**, where it is tested on every OS. Anything that
crosses the Tauri boundary is defined once, in `crates/types` (serde only — it compiles
to wasm) — **except `AppConfig`**, which is deliberately two types: the app's
(`src-tauri/src/config/app_config.rs`, owns the file on disk and the real defaults) and
the ui's (`src/ui_types.rs`, the UI's view of it). Architecture decision, Kade
2026-09-29 — do not merge them. A field added to one must be added to the other with
the same name, or it will not cross the boundary.

---

## Tech Stack

- **Frontend:** Leptos 0.8 (CSR) → wasm via **Trunk**; Tailwind 4 + daisyUI through
  `npx @tailwindcss/cli` (Trunk pre-build hook, `cmd /c` — Windows shell).
- **Backend:** Tauri 2 (`unstable`, tray, global-shortcut, fs, shell, opener).
- **Capture:** raw socket with `SIO_RCVALL` (`windows-sys`,
  `src-tauri/src/services/sniffer/network.rs`). Needs **Administrator**. Port 5003
  carries chat. No Npcap / WinDivert (removed 2026-09-29; capture never used them).
- **Translation:** llama.cpp server (Vulkan build, downloaded at runtime from this
  repo's releases) on `127.0.0.1:8080`, or a free port if 8080 is taken; only the PID the
  app started is ever killed. Pre/post-processing in `crates/core/src/text.rs`.
- **Remote metadata:** a public gist (`downloader/gist.rs`) carries model/dictionary
  versions and the custom dictionary; the app's own update comes from the newest stable
  release's `latest.json` (see *Stable releases*), checked against signing keys built into
  the app. The gist's `app` entry is ignored by the app and kept only for copies that
  predate the signed updater (the 0.6.0 bridge release). Public URLs, not secrets.
- **Packaging:** `package.bat` → `cargo tauri build` → NSIS installer in `dist/`.
  Test builds: a merge into `rc` publishes a plain exe as a GitHub prerelease (see
  *Release candidates*). The app version is `[workspace.package] version` in `Cargo.toml`.

---

## Repository Layout

```
.claude/              graft wiring (hooks, helpers), skills/: graft, workflow-control, ui-preview
.memory/              working memory; see .memory/README.md
.github/workflows/    CI — the gates, per OS
justfile              the gates as commands
crates/core/           resonance-core — pure logic, tested on any OS
  src/protocol/         port 5003: stream framing (framing.rs) + protobuf-style decoding
  src/capture/          ChatPipeline: raw IPv4/TCP bytes → dedup/blocked ChatMessages
  src/text.rs           translation pre/post-processing, Dictionary, emotes, romaji
  src/history.rs        ChatHistory (backend chat log) + load_recent (daily chat_logs reload)
  src/workers.rs        worker decisions: translator on/off/restart, stale jobs, port
  src/download.rs       download checks: HTTPS, length + SHA-256, progress, versions
  src/sniffer_net.rs    sniffer network setup: per-exe firewall rule name, adapter pick (route first)
crates/llama/          resonance-llama — llama-server HTTP client (/completion, /health), any OS
crates/types/          resonance-types — DTOs shared across the Tauri boundary (serde only)
src/                  ui crate (resonance-stream-ui)
  app/                  App shell; actions.rs (save_config, clear_history),
                          hydration.rs (start-up load), setup_flow.rs (first-run wizard)
  store.rs              AppSignals (app-wide signals, AppSignals::new) + AppActions
  config_signals.rs     ConfigSignals: the signals that mirror AppConfig; to_config() / apply()
  status_signals.rs     ServiceSignals / SetupSignals / UpdateSignals: backend status, wizard, update dialogs
  view_signals.rs       ChatSignals / UiSignals: chat list, system log, scroll + unread, open dialogs
  chat_view.rs          chat list: per-tab views + limits (ChatStore), filter, paging -- pure, host-tested
  components/           views; settings/ is one file per settings section
  hooks/                backend event, config and tray wiring
  ui_types.rs           ui-only types (AppConfig) + re-export of resonance-types
src-tauri/            app crate (resonance-stream, lib resonance_stream_lib)
  src/lib.rs            module list, crate-root re-exports, run() — start-up wiring only
  src/events.rs         inject_system_message / store_and_emit: emit to UI + keep history
  src/commands.rs       history + translator commands; window.rs, tray.rs, shortcut.rs
  src/protocol/types.rs AppState and backend-only types; re-exports resonance-types
  src/services/         sniffer/ (sockets, workers) translator/ (llama server) downloader/
  src/config/ src/io/   config + metadata persistence, archive writer
graft/                graft's generated cards — GITIGNORED, regenerable (`graft build`)
style/ public/        CSS source, static assets
runbook/              the runbook: notebooks, helpers, dry-run tests -- docs, see Guardrails
```

---

## Using graft (the repo is indexed)

Reach for graft before grep/read — see `.claude/skills/graft/SKILL.md`.

- **Before moving, renaming or splitting a symbol:** `graft callers <sym> --depth all`.
  Editing the primary file and stopping is the classic miss.
- `graft skeleton <file>` before reading a large file whole.
- After structural changes graft rebuilds via the PostToolUse hook; `graft build` if stale.

---

## Conventions (app crate)

- **Tauri commands are `pub` and reach `generate_handler!` through the crate-root
  `pub use <module>::*`.** A new command module needs both. A private `use` of the same
  name in a module shadows its glob re-export (rustc warns) — the item then silently
  stops being exported; call it by path instead.

- **Favorites change only through `save_favorites`** (`FavoritesState`, then the
  `favorites-changed` event reaches every window); `save_config` keeps the stored ones and
  ignores the ones in its payload. A popup window (`open_popup`) saves only what it owns,
  never the whole config -- its copy of the settings may be older than the main window's.

## Conventions (ui crate)

- **State lives in `AppSignals` (context), not in component locals**, when more than one
  component touches it. Components read it with `use_context::<AppSignals>()` and reach a
  signal through its group: `signals.config` (settings), `.service`, `.setup`, `.updates`,
  `.chat`, `.ui`. A new signal goes in the group it describes.
- **A component under `<Show>` is re-created each time it shows.** State that must
  survive closing a modal (typed input, an `Action`'s last result) is created by the
  parent outside the `<Show>` and passed down as props — see `components/settings/`.
- Helpers that need many signals take `signals: AppSignals` and destructure only the
  fields they use (`let ChatSignals { a, set_b, .. } = signals.chat;`).
- **A setting that lives in `config.json` is a field of `AppConfig` (ui) and a signal of
  `signals.config` (`ConfigSignals`, named like the field).** Adding one means adding it
  to `ConfigSignals` -- `to_config` / `apply` list every field, so forgetting is a compile
  error -- and to the app's `AppConfig`. Its load-time quirks (a saved value that is
  clamped or replaced) live in `apply`, not in `hydration.rs`.

## Guardrails

- **Plan first.** Do not modify modules, components, manifests or CI on the first turn
  of a task. Present an impact analysis (graft `callers` output is the evidence) and
  wait for explicit confirmation. See `.claude/skills/workflow-control/SKILL.md`.
- **Refactors do not change behaviour.** A move/split commit changes no logic, no UI,
  no wire format. `ChatMessage`/`SystemMessage` serialize as camelCase across the
  Tauri boundary — renaming a field is a protocol change, not a refactor.
- **Zero hardcoded credentials.** No tokens or keys in committed files. The gist and
  release URLs are public and fine.
- **TDD for every task flow.** Test first: write the failing unit test that pins the
  wanted behaviour, run it and see it fail for the right reason, then write the code
  that makes it pass, then refactor with the tests green. A bug fix starts with a
  test that reproduces the bug. Pure logic is tested in `crates/core` (or the ui's pure
  modules), so it runs on every OS. If a change cannot be unit-tested (Tauri/Windows
  glue), say so in the commit body. `just coverage` shows what the tests do not reach.
- **The runbook is docs.** `runbook/` -- the Jupyter notebooks Kade runs by hand on his
  Windows PC, their helpers, dry-run tests and fixtures -- changes no app logic, so it is
  treated like a doc: **no CI job checks it** (do not add one), **no gate** for it in the
  table, no test-first requirement, no `NOT VERIFIED` line for it. A PR that only touches
  `runbook/` (and memory) is an ordinary `claude/*` PR; CI still runs on it, because
  auto-merge needs a green run, but nothing in CI tests the runbook. A courtesy, not a gate:
  after changing its helpers run `cd runbook && python -m pytest -q` (about 3 minutes).
  The runbook's real proof is Kade's run on Windows, and the report he pastes back says
  what is broken. Fix a notebook in the same PR as the code change it follows.
- **Auto-correction restraint.** Self-correct at most **2** times, then stop and ask.
- **Never report a gate as passed when it could not run.** A Linux session cannot
  build `src-tauri/`; say so, and leave it to the Windows CI job.
- **A blocked host is asked for, not worked around.** A session can only reach the hosts
  in its environment's trusted-host list. When a build, download or test needs a host
  that is blocked (e.g. `Lindera.dev`, which the furigana dictionary's build script
  downloads from), stop and ask Kade to add it -- name the host and what needs it. Do not
  substitute a mirror, re-encode another copy, patch a dependency or stub the result to
  get past it. Until it is added, the gate that needs it is reported as not run.
  `Lindera.dev` is trusted since 2026-10-03.

---

## Definition of Done

0. **Test first.** New behaviour or a bug fix has its failing unit test before its code.
1. **Run the gate for every part touched** (table above). `cargo fmt` is part of it.
2. **Behaviour check where a gate cannot see it.** UI changes are screenshotted in a
   browser with the `ui-preview` skill (`.claude/skills/ui-preview/`, runs on Linux),
   and need a manual run (`cargo tauri dev`, Windows, as Administrator); say in the
   commit body which of the two was done.
3. **Record the outcome in the memory tree.** `MEMORY.md` is an index under ~40 lines —
   update its *Now* section. Detail goes in `.memory/` (see its README).
4. **Push the branch and open the PR** — see *Version Control*; CI merges it when green.

---

## Version Control

**One task, one branch, one PR.** Claude runs the whole flow without being asked.

1. **Start.** Every new task gets its own branch from an up-to-date `main`:
   `git checkout main && git pull && git checkout -b claude/<short-task-name>`.
   Never commit to `main`. A follow-up to a merged task is a new task: new branch.
2. **During the task, commit freely** -- as many local commits as help. Unpushed history may
   be tidied (`git commit --amend`, or `git reset --soft <base>` + one commit to squash).
   Never rewrite history that is already pushed.
3. **Finish = test, then push.** When the task is done, run the gate for every part
   touched (table at the top; `cargo fmt` included) and fix failures. Only a green
   local gate is pushed: `git push -u origin claude/<name>`. A gate that could not run
   here is named in the last commit body (`NOT VERIFIED: app gate -- no Windows
   toolchain in this session`) and left to CI.
4. **Open the PR** against `main` (check for a PR template first). Do not merge it by
   hand: `.github/workflows/auto-merge.yml` merges it and deletes its `claude/*` branch (never any other branch) once the CI
   workflow passes on the PR's latest commit. If CI fails, fix on the same branch and
   push again -- the run for the new commit decides. Never skip, disable or edit a
   test/gate to get green.

### Several tasks in one session

When a session is given a series of tasks (or one task split into steps, one PR each):

1. **One PR at a time, in order.** Finish a task (gate green, pushed), open its PR, then
   **wait until the PR is merged** -- the auto-merge workflow merges it once CI passes on
   the latest commit. Do not start the next task, or push anything for it, before that.
2. **CI failed?** Fix it first, on the same branch, and push again. The run for the new
   commit decides. Never skip, disable or edit a test/gate to get green. Retry until the
   PR merges; if a failure is not this PR's (red on `main` too), say so on the PR.
3. **Before the next task, check it is really done:** the PR is closed as *merged* and its
   `claude/*` branch is gone. Then start from `main` again: `git fetch origin main &&
   git checkout -B claude/<next> origin/main`. A later task never stacks on an unmerged one.
4. Waiting is done with the PR event subscription (`subscribe_pr_activity`) and a
   check-in (`send_later`), not with `sleep` loops. Update `MEMORY.md` in each task's own
   branch, so a merged task never leaves the index behind.

```bash
git status            # check BEFORE -A, never after
git add -A && git commit
```

- **Commit subject states the point of the change**, not the files touched
  (`Protocol decoding builds on any OS now -- moved out of the Windows crate`,
  not `move files`). The body says what changed, why, and **what is verified vs open**.
- `MEMORY.md` and `.memory/` updates go in the branch, before the push.
- **Claude never commits work it did not do.** Pre-existing changes stay untouched.
- Only `claude/*` branches auto-merge, and only PRs into `main`: a PR into `rc` (or any
  other branch) stays open until a person merges it. `workflow_run` workflows are read from `main`, so a
  change to `auto-merge.yml` itself takes effect after it has been merged once.
- Merges made by the workflow use `GITHUB_TOKEN`, which does not start a `push` run on
  `main`; the PR's own run is the gate.

### Test branches (`test/*`)

The runbook lives on `main`, in `runbook/` (see *The runbook is docs*); the old
`test/w1-*` branches are an archive and get no updates. A `test/<job>` branch is now only
for a **throw-away** experiment on Kade's Windows PC that does not belong in `main`.
**It is never merged into `main` and never opens a release**: cut it from `main`, push it,
and Kade checks it out. No PR is needed; if one is opened into `main`,
`.github/workflows/test-branch-guard.yml` fails it (helper `branch-guard.sh`, tested in CI).
A finding from a run becomes a normal task on a `claude/*` branch, test first when it is
app logic; the `test/*` branch itself stays out of `main`'s history.

### Release candidates (`rc`)

A change that needs a run on Kade's Windows PC before `main` goes through `rc`:

1. Work on `candidate/<feature>` (from `main`), gate green, push. Open a PR **into `rc`**
   -- Kade or another maintainer merges it by hand (auto-merge never touches it).
2. The merge starts `.github/workflows/release-candidate.yml`: CI's gates, then a
   Windows build of the plain exe (`tauri build --no-bundle`, no installer), then the
   **real-app smoke test** (`bridge-smoke.yml`: that exe started on a hosted Windows runner
   and driven over the test bridge, eight pipelines; a red row stops the publish), then a
   GitHub **prerelease** `v<version>-rc.<feature>` (`.2`, `.3` ... for a repeat
   build of the same branch) with the exe, `SHA256SUMS.txt`, and Korean notes: how to
   run it, the merged PR's description, the commits not yet on `main`, and their
   `NOT VERIFIED` lines. Only the newest 5 candidates (and their tags) are kept.
   Tag / notes logic: `.github/scripts/rc-lib.sh`, tested by `rc-lib.test.sh` (CI).
3. After Kade's test, the **same branch** goes to `main` in its own PR. `rc` is never
   merged into `main`; it is kept current by merging `main` into it.

Prereleases never become "Latest", and the app's update check reads the gist, not
GitHub releases, so users never see a candidate. The candidate exe shares the
installed app's data folder (same identifier): config, model, chat logs.

### Stable releases (signed plain exe)

An app update is installed only if one of the keys built into the app
(`TRUSTED_UPDATE_KEYS`, `crates/core/src/update_signature.rs`) signed it for the
announced version -- so a stable release is built and signed by
`.github/workflows/release.yml`, never by hand: bump `[workspace.package] version`, write
`release-notes/v<version>.md` (copy `release-notes/TEMPLATE.md`), merge to `main`, then
push the tag `v<version>` on it. The workflow gates, runs the same
real-app smoke test on a separate test-flag build of that commit (the shipped exe has no
bridge; a red row stops the publish), builds
the plain exe, signs it with the `TAURI_SIGNING_PRIVATE_KEY` secret, checks the
signature the way the app will (`examples/verify_update.rs`) and publishes the exe,
`<exe>.sig` and `latest.json` (the update feed). The private keys are never
committed; the backup key stays offline. Tag / feed helpers:
`.github/scripts/release-lib.sh` (tests in CI).

**Never publish a stable release by hand.** Every installed app reads
`releases/latest/download/latest.json`; a hand-made release (no `latest.json`, another
exe) or a candidate that is not a prerelease becomes "latest" and updates silently stop
-- the app just finds no update. `release.yml` reads the live feed back after publishing,
and `release-feed-check.yml` watches it (daily, and when a person publishes, edits or
deletes a release); both run `.github/scripts/check-release-feed.sh`. A red run means
users get no update: delete the hand-made release or mark it prerelease, then run the
workflow again.

**Release notes: simple for users, detailed for maintainers.** `release-notes/v<version>.md`
has two layers. Above the line `## 개발자용 상세` is the **user summary**: a few plain
lines (about 10, at most 12) in Korean, in everyday words -- no commit subjects, PR
numbers, file or function names, no English. It is what the app's update dialog shows
(`latest.json`'s `notes`) and the top of the release page. Below that line is the
**maintainer detail** (technical, any length, English is fine); the release page puts it,
with the commit list since the previous stable tag, in one collapsed block, and the app
never shows it. `release.yml` refuses to start without the file, or when the summary is
empty, over 12 lines, has a line without Korean, or still has the `<<작성>>` placeholder
(`release_notes_problem`, tested in `release-lib.test.sh`). The update dialog's notes box
also scrolls past a fixed height, so a long note can never push its buttons off screen.
Candidate (`rc`) notes are for the tester and keep their own format.

### Never commit
- Secrets, `.env`.
- Build output: `target/`, `dist/`, `style/output.css`, `*.exe`.
- `graft/` (regenerable), `node_modules/`, IDE folders.
- A half-applied or unformatted tree "to save progress". Use a branch.
