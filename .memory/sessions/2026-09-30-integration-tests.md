# 2026-09-30 — integration tests: mock game server + mock llama-server

Branch `claude/integration-test-harness`. Kade's calls: HTTP client in its own crate
(`crates/llama`, not core); timeouts 2 s connect / 30 s request; an empty model reply
is an error; integration tests first, real capture (W2) after.

## What exists now
- `crates/llama` (resonance-llama): `translate_text`, `health_ok`, `client()` with
  timeouts. Part of `just core-check`.
- `resonance_core::text::translate_masked`: cache -> model -> postprocess, model as a
  closure; `process_translation_job` calls it.
- `crates/llama/tests/`: `support/game_server.rs` (encoder, framing, TCP segmenting,
  IPv4/TCP packets), `support/llama_server.rs` (scripted /health + /completion,
  records requests), `support/harness.rs` (**mirrors** the app's sniffer dispatch and
  `process_translation_job` without Tauri -- keep in step with them).
  `capture.rs` 27, `llama.rs` 18, `scenarios.rs` 8 tests.

## Found
- **Framing drops lines around rich messages** (W2). The text chunk `0x0A len text`
  has a root's shape. (a) A rich line cut right after its text is lost, even on a
  connection with a learned header. (b) When no segment of a burst ends on a frame
  boundary, a segment ending inside a rich line resolves as that false root and the
  assembler throws away everything it was holding (8 lines in the pinned case).
  Both pinned in `capture.rs` as today's behaviour; a framing fix flips them.
  **Fixed by W2 (length-based framing); both tests now assert the lines are shown.**
  It was the mock's guessed header, not observed in a real capture.
  Random coalescing on frame boundaries and random splits of plain lines are fine
  (500 seeded rounds each).
- **No HTTP timeout** on the translator's client: a hung llama-server stalled the
  worker forever. Fixed (test first).
- Wrong turns: my fixtures assumed romaji `Taro`/`azururu`; kakasi gives `Tarou`,
  `Azururu`.

## Not covered
Real llama-server process, raw socket capture, Tauri emit / ledger / archive, the
pending timeout inside `ChatPipeline` (no clock parameter; `FrameAssembler` tests
cover it). Observed, not changed: the app translates blocked users' lines too
(`dispatch_pipeline_actions` does not check `is_blocked`).
