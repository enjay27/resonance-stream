# Gates for resonance-stream. `just check` runs every gate this OS can run.
# See CLAUDE.md for which gate applies to which part of the repo.

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default: check

# Every gate the current OS can run (the app crate builds on Windows only).
check: fmt-check ui-check
    {{ if os() == "windows" { just_executable() + " app-check" } else { "echo 'app gate skipped: src-tauri builds on Windows only'" } }}

# Formatting, whole workspace.
fmt-check:
    cargo fmt --all -- --check

fmt:
    cargo fmt --all

# ui part: Leptos frontend, compiled for the browser target.
ui-check:
    cargo check -p resonance-stream-ui --target wasm32-unknown-unknown

# Needs a `dist/` folder: tauri::generate_context! checks frontendDist exists.

# app part: Tauri backend, check + tests. Windows only.
app-check:
    cargo check -p resonance-stream
    cargo test -p resonance-stream
