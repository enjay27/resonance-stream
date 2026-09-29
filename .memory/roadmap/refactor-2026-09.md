# Refactor — 2026-09 (graft-guided)

Plan agreed 2026-09-29. One phase, one commit. Push is Kade's.

| # | phase | done when |
|---|---|---|
| 0 | CLAUDE.md, MEMORY.md, `.memory/`, workflow-control skill | files exist, index < 40 lines |
| 1 | `justfile` gates + GitHub Actions (ui on Linux, app on Windows); `cargo fmt` baseline | `just check` green on Linux; CI file valid |
| 2 | `crates/core`: protocol decoding, text processing, shared DTOs; ui + app depend on it | core tests run on Linux; one definition of `ChatMessage`/`SystemMessage` |
| 3 | split `src-tauri/src/lib.rs` into commands / tray / state; `run` only wires | no behaviour change; app gate (CI) |
| 4 | split `Settings` (920-line fn) and `App` (950-line fn) into section components | ui gate green; UI unchanged (manual run on Windows) |
| 5 | hygiene: commit `Cargo.lock`, one version source, `package.json` metadata, `stream_traacker.rs` typo, template leftovers, stale docs | gates green |

Baseline numbers (2026-09-29, `graft map`): 48 files, 274 symbols, 265 edges.
Largest: `src/app.rs` 971 lines (one fn), `src/components/settings.rs` 933 (one fn),
`src-tauri/src/protocol/parser.rs` 638, `src-tauri/src/lib.rs` 517.
Hotspots: `inject_system_message` 19 callers, `read_varint` 14, `skip_field` 9.
`cargo fmt --check`: 8 files differed (fixed in 8b15f92, listed in `.git-blame-ignore-revs`).
