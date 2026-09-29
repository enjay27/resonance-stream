# Known bugs (from the 2026-09-29 code review)

Full evidence, line numbers and fix sketches: [`docs/code-review-2026-09-29.md`](../../docs/code-review-2026-09-29.md).
Close an item by deleting its line in the commit that fixes it.

**Reproduced on Linux** (probe crate linking `resonance-core`, synthetic packets):
- **B5** `[profile.release]` in `src-tauri/Cargo.toml` is ignored (non-root member); cargo warns on every build.

**From reading the code, not run** (app crate, Windows only):
- **B7** Model re-download deletes the old model before downloading.
