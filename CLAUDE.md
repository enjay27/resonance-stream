# Agent Operating Rules — resonance-stream

Resonance Stream is a Windows desktop app: it sniffs Blue Protocol: Star Resonance
chat packets (raw socket, no client hooking), translates Japanese chat to Korean
through a local llama.cpp server, and shows it in a Tauri overlay.

It is one Cargo workspace in four crates, three parts. **Which part you touch decides which
gate applies.** That is the most important thing on this page.

| tree | part | builds on | gate |
|---|---|---|---|
| `crates/core/` `crates/types/` | **core** — packet → chat pipeline, protocol decoding, translation text processing; DTOs shared by app and ui | any OS | `just core-check` |
| `src/` | **ui** — Leptos 0.8 CSR frontend (wasm) | any OS | `just ui-check` |
| `src-tauri/` | **app** — Tauri 2 backend: sockets, translator server, downloader, windows, tray | **Windows only** | `just app-check` (Windows) · `just app-cross-check` (Linux, compile only) |

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
  repo's releases) on `127.0.0.1:8080`; pre/post-processing in `crates/core/src/text.rs`.
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
crates/core/           resonance-core — pure logic, tested on any OS
  src/protocol/         port 5003: packet reassembly + protobuf-style decoding of chat
  src/capture/          ChatPipeline: raw IPv4/TCP bytes → dedup/blocked ChatMessages
  src/text.rs           translation pre/post-processing, dictionary, romaji
crates/types/          resonance-types — DTOs shared across the Tauri boundary (serde only)
src/                  ui crate (resonance-stream-ui)
  app/                  App shell; actions.rs (save_config, clear_history),
                          hydration.rs (start-up load), setup_flow.rs (first-run wizard)
  store.rs              AppSignals (all app-wide signals, AppSignals::new) + AppActions
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

## Conventions (ui crate)

- **State lives in `AppSignals` (context), not in component locals**, when more than one
  component touches it. Components read it with `use_context::<AppSignals>()`.
- **A component under `<Show>` is re-created each time it shows.** State that must
  survive closing a modal (typed input, an `Action`'s last result) is created by the
  parent outside the `<Show>` and passed down as props — see `components/settings/`.
- Helpers that need many signals take `signals: AppSignals` and destructure only the
  fields they use (`let AppSignals { a, set_b, .. } = signals;`).

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
- Build output: `target/`, `dist/`, `style/output.css`, `*.exe`.
- `graft/` (regenerable), `node_modules/`, IDE folders.
- A half-applied or unformatted tree "to save progress". Use a branch.
