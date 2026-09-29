# Known bugs (from the 2026-09-29 code review)

Full evidence, line numbers and fix sketches: [`docs/code-review-2026-09-29.md`](../../docs/code-review-2026-09-29.md).
Close an item by deleting its line in the commit that fixes it.

**Reproduced on Linux** (probe crate linking `resonance-core`, synthetic packets):
- **B2** `strip_application_header(5003)` accepts a segment only if a root ends exactly at its end: coalesced messages keep only the last, split messages are dropped, `PacketBuffer` reassembly is unreachable. Confirm the real framing with a captured fixture first.
- **B5** `[profile.release]` in `src-tauri/Cargo.toml` is ignored (non-root member); cargo warns on every build.

**From reading the code, not run** (app crate, Windows only):
- **B4** `save_config` starts the translator worker twice on off→on (two blocks, `app_config.rs`).
- **P1** `load_config` (disk read + JSON parse) runs for every packet the raw socket receives.
- **P3** Translator holds `nickname_cache` lock across the HTTP call → sniffer stalls.
- **B6** Dictionary sync/save does not reach the running translator.
- **B7** Model re-download deletes the old model before downloading.
