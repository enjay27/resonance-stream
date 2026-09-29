# Agent Operating Rules — resonance-stream

Resonance Stream is a Windows desktop app: it sniffs Blue Protocol: Star Resonance
chat packets (WinDivert, no client hooking), translates Japanese chat to Korean
through a local llama.cpp server, and shows it in a Tauri overlay.

It is one Cargo workspace in three parts. **Which part you touch decides which
gate applies.** That is the most important thing on this page.

| tree | part | builds on | gate |
|---|---|---|---|
| `src/` | **ui** — Leptos 0.8 CSR frontend (wasm) | any OS | `just ui-check` |
| `src-tauri/` | **app** — Tauri 2 backend: sniffer, translator, downloader, windows, tray | **Windows only** | `just app-check` |

`just check` runs `fmt-check` plus every gate the current OS can run (`pip install
rust-just` or `cargo install just`). CI (`.github/workflows/ci.yml`) runs the ui gate
on Linux and the app gate on `windows-latest`, on every push and PR.

---

## Tech Stack

- **Frontend:** Leptos 0.8 (CSR) → wasm via **Trunk**; Tailwind 4 + daisyUI through
  `npx @tailwindcss/cli` (Trunk pre-build hook, `cmd /c` — Windows shell).
- **Backend:** Tauri 2 (`unstable`, tray, global-shortcut, fs, shell, opener).
- **Capture:** WinDivert (`windivert` crate, vendored) + raw-socket fallback
  (`windows-sys`). Needs **Administrator**. Port 5003 carries chat.
- **Translation:** llama.cpp server (Vulkan build, downloaded at runtime from this
  repo's releases) on `127.0.0.1:8080`; pre/post-processing in `translator/processor.rs`.
- **Remote metadata:** a public gist (`downloader/gist.rs`) carries app/model/dictionary
  versions and the custom dictionary. Public URLs, not secrets.
- **Packaging:** `package.bat` → `cargo tauri build` → NSIS installer in `dist/`.

---

## Repository Layout

```
.claude/              graft wiring (hooks, helpers), skills/: graft, workflow-control
.memory/              working memory; see .memory/README.md
.github/workflows/    CI — the gates, per OS
justfile              the gates as commands
src/                  ui crate (resonance-stream-ui)
  components/           views; hooks/ (event + config wiring); store.rs (signals)
src-tauri/            app crate (resonance-stream, lib resonance_stream_lib)
  src/protocol/         packet reassembly + protobuf-ish decoding of chat
  src/services/         sniffer/ translator/ downloader/
  src/config/ src/io/   config + metadata persistence, archive writer
graft/                graft's generated cards — GITIGNORED, regenerable (`graft build`)
style/ public/        CSS source, static assets
```

---

## Using graft (the repo is indexed)

Reach for graft before grep/read — see `.claude/skills/graft/SKILL.md`.

- **Before moving, renaming or splitting a symbol:** `graft callers <sym> --depth all`.
  Editing the primary file and stopping is the classic miss.
- `graft skeleton <file>` before reading a large file whole.
- After structural changes graft rebuilds via the PostToolUse hook; `graft build` if stale.

---

## Guardrails

- **Plan first.** Do not modify modules, components, manifests or CI on the first turn
  of a task. Present an impact analysis (graft `callers` output is the evidence) and
  wait for explicit confirmation. See `.claude/skills/workflow-control/SKILL.md`.
- **Refactors do not change behaviour.** A move/split commit changes no logic, no UI,
  no wire format. `ChatMessage`/`SystemMessage` serialize as camelCase across the
  Tauri boundary — renaming a field is a protocol change, not a refactor.
- **Zero hardcoded credentials.** No tokens or keys in committed files. The gist and
  release URLs are public and fine.
- **Auto-correction restraint.** Self-correct at most **2** times, then stop and ask.
- **Never report a gate as passed when it could not run.** A Linux session cannot
  build `src-tauri/`; say so, and leave it to the Windows CI job.

---

## Definition of Done

1. **Run the gate for every part touched** (table above). `cargo fmt` is part of it.
2. **Behaviour check where a gate cannot see it.** UI changes need a manual run
   (`cargo tauri dev`, Windows, as Administrator); if not done, say so in the commit body.
3. **Record the outcome in the memory tree.** `MEMORY.md` is an index under ~40 lines —
   update its *Now* section. Detail goes in `.memory/` (see its README).
4. **Commit the task as one change, automatically** — see *Version Control*.

---

## Version Control

**One task, one commit**, made by Claude without being asked, as soon as the gate is
green. Not per file edit, not batched across unrelated tasks.

```bash
git status            # check BEFORE -A, never after
git add -A && git commit
```

- **Subject states the point of the change**, not the files touched
  (`Protocol decoding builds on any OS now -- moved out of the Windows crate`,
  not `move files`). The body says what changed, why, and **what is verified vs open**.
- **If a gate could not run**, commit anyway and say so in the body:
  `NOT VERIFIED: app gate — no Windows toolchain in this session`.
- `MEMORY.md` and `.memory/` updates go in the **same commit** as the code they describe.
- **Claude never commits work it did not do.** Pre-existing changes stay untouched.
- **`git push` is Kade's.** Auto-commit is local history; publishing is a separate decision.

### Never commit
- Secrets, `.env`.
- Build output: `target/`, `dist/`, `style/output.css`, `*.exe`, `lib/` (Npcap/WinDivert SDKs),
  `WinDivert*.{dll,lib,sys}`.
- `graft/` (regenerable), `node_modules/`, IDE folders.
- A half-applied or unformatted tree "to save progress". Use a branch.
