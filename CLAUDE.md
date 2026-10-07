# Agent Operating Rules — resonance-stream

Resonance Stream is a Windows desktop app: it sniffs Blue Protocol: Star Resonance
chat packets (raw socket, no client hooking), translates Japanese chat to Korean
through a local llama.cpp server, and shows it in a Tauri overlay.

One Cargo workspace in five crates, three parts, plus the runbook (treated as docs).
**Which part you touch decides which gate applies** -- the most important thing on this page.

Follow the `kade-workflow` skill (plan first, test first, gates, commit and PR style, context
budget). Where it and this file differ, this file wins. Current state: open issues by label
(`status:now|next`, `verify:needs-kade|not-verified`), index @MEMORY.md

| tree | part | builds on | gate |
|---|---|---|---|
| `crates/core/` `crates/llama/` `crates/types/` | **core** — packet → chat pipeline, protocol decoding, translation text processing; llama-server HTTP client; DTOs shared by app and ui | any OS | `just core-check` |
| `src/` | **ui** — Leptos 0.8 CSR frontend (wasm); pure modules unit-tested on the host | any OS | `just ui-check` |
| `src-tauri/` | **app** — Tauri 2 backend: sockets, translator server, downloader, windows, tray | **Windows only** | `just app-check` (Windows) · `just app-cross-check` (Linux, compile only) |
| `runbook/` | **runbook** — Jupyter notebooks + Python helpers that Kade runs by hand on Windows (`runbook/README.md`) | any OS (dry runs) | **none — treated as docs** (see `.claude/rules/runbook.md`) |

`just check` (`pip install rust-just`) runs `fmt-check`, then every gate this OS can run. On Linux
the app gate is a **compile-only** cross-check against `x86_64-pc-windows-gnu` (rustup target +
`gcc-mingw-w64-x86-64`), so the app's own tests run only on Windows. CI (`ci.yml`) runs core + ui
on Linux and the full app gate on `windows-latest`, on every push and PR.

**New pure logic goes in `crates/core`**, where it is tested on every OS. Anything that
crosses the Tauri boundary is defined once, in `crates/types` (serde, serde_with — it
compiles to wasm) -- `AppConfig`, the settings file's type, too (`crates/types/src/app_config.rs`;
the app owns the file and the real defaults, the ui re-exports the same type).

## Tech Stack

- **Frontend:** Leptos 0.8 (CSR) → wasm via Trunk; Tailwind 4 + daisyUI (Trunk pre-build hook, `cmd /c`).
  **Backend:** Tauri 2 (`unstable`, tray, global-shortcut, fs, shell, opener).
- **Capture:** raw socket with `SIO_RCVALL`, needs **Administrator**; port 5003 carries chat. No Npcap / WinDivert.
- **Translation:** llama.cpp server (Vulkan, downloaded at runtime) on `127.0.0.1:8080` or a free port;
  only the PID the app started is ever killed. Pre/post-processing in `crates/core/src/text.rs`.
- **Updates and metadata:** signed stable-release feed and signed metadata; detail in `.claude/rules/app.md`.
- **Packaging:** `package.bat` → `cargo tauri build` → NSIS in `dist/`; version: `[workspace.package]` in `Cargo.toml`.

## Repository Layout (top level; per file: `.claude/rules/{core,ui,app}.md` and graft)

```
.claude/              graft wiring, context-guard hook, rules/ (path-scoped), skills/: graft, release, ui-preview, workflow-control
.memory/              working memory; see .memory/README.md
.github/ justfile     CI (gates per OS; release, rc, metadata workflows); the gates as commands
crates/               core, llama, types — pure logic, tested on any OS
src/                  ui crate (resonance-stream-ui)
src-tauri/            app crate (resonance-stream, lib resonance_stream_lib)
metadata/ release-notes/  signed metadata source; release notes per version
docs/                 architecture review, decisions log, security model, testing
runbook/              notebooks Kade runs on Windows -- docs, see .claude/rules/runbook.md
graft/ style/ public/ graft's cards (GITIGNORED, `graft build`); CSS source; static assets
```

## Using graft (the repo is indexed)

Reach for graft before grep/read — see `.claude/skills/graft/SKILL.md`.
- **Before moving, renaming or splitting a symbol:** `graft callers <sym> --depth all`.
  Editing the primary file and stopping is the classic miss.
- `graft skeleton <file>` before reading a large file whole; after structural changes, `graft build` if stale.

## Guardrails

- **Refactors do not change behaviour.** `ChatMessage`/`SystemMessage` serialize as camelCase
  across the Tauri boundary — renaming a field is a protocol change, not a refactor.
- **Zero hardcoded credentials.** No tokens or keys in committed files; gist and release URLs are public.
- **Where tests go.** Pure logic is tested in `crates/core` (or the ui's pure modules), so it
  runs on every OS. `just coverage` shows what the tests do not reach.
- **The runbook is docs**: no CI job, no gate, no test-first requirement for `runbook/`; the full rule is `.claude/rules/runbook.md`.
- **A Linux session cannot build `src-tauri/`**; say so, and leave it to the Windows CI job.
- **Trusted hosts:** `Lindera.dev` (the furigana dictionary's build script downloads from it).
- **This file ≤ 100 lines; each `.claude/rules/*.md` ≤ 80 lines with `paths:`** (`claude-md-check.sh`, CI).

## Definition of Done

0. **Test first.** New behaviour or a bug fix has its failing unit test before its code.
1. **Run the gate for every part touched** (table above). `cargo fmt` is part of it.
2. **Behaviour check where a gate cannot see it.** UI changes: `.claude/rules/ui.md`.
3. **Record state on the issue** (`status:` / `verify:` labels; a follow-up gets an issue); `MEMORY.md` only for what has none.
4. **Push the branch and open the PR**; CI merges it when green.

## Version Control

**One task, one branch, one PR**, run by Claude without being asked.

- Branch from an up-to-date `main`: `git fetch origin main && git checkout -B claude/<task> origin/main`.
  Never commit to `main`; a follow-up is a new branch. Never rewrite history that is already pushed.
- `.github/workflows/auto-merge.yml` merges a green PR and deletes its `claude/*` branch. Never
  merge by hand, and never skip, disable or edit a test/gate to get green. A failure that is
  not this PR's (red on `main` too) is said on the PR.
- **Several tasks:** one PR at a time; start the next only when the last is *merged* and its
  branch is gone. Wait with `subscribe_pr_activity` and a `send_later` check-in, not `sleep` loops.
- Only `claude/*` PRs into `main` auto-merge; a PR into `rc` (or anywhere else) waits for a person.
  `workflow_run` workflows are read from `main`: an `auto-merge.yml` change acts after its own merge.
  Workflow merges use `GITHUB_TOKEN` (no `push` run on `main`); the PR's own run is the gate.
- **Test branches, release candidates, stable releases:** the `release` skill; the rules for release
  files (never publish by hand, release notes) are in `.claude/rules/release.md`.
- **Never commit:** secrets, `.env`; build output (`target/`, `dist/`, `style/output.css`, `*.exe`);
  `graft/`, `node_modules/`, IDE folders; a half-applied or unformatted tree "to save progress".
