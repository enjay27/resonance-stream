# 2026-09-29 — fixing the code review, test-first

Kade asked for every item of `docs/code-review-2026-09-29.md` to be fixed with TDD, in
the report's order. Seven commits, `fbfd552` onward; the report's *Status* maps items
to commits.

## How
- Pure logic moved where Linux can test it: `crates/core` (`protocol/framing`,
  `history`, `workers`, `download`, `text::Dictionary`) and `src/chat_view.rs` (the UI
  crate's host tests run with plain `cargo test`; `just ui-check` now includes them).
- Each change: test first, seen failing (compile-red first, then `todo!()` stubs for a
  behavioural red), then the implementation.
- App wiring can't be tested here: the `cdylib` does not link with mingw ("export
  ordinal too large"), and there is no wine. Pushed after step 4 so Windows CI ran app
  build and tests (run 36598671154, green).

## Wrong turns (kept on purpose)
- The first fuzz test passed on the buggy parser: random bytes almost never form a
  10-byte max varint. It now splices one in, and fails on the old code.
- The first "learned header" framing test passed with learning disabled (my crafted
  header did not chain). Found by mutating the code; fixed so the mutation fails.
- The number-units test first asserted "no digits", but placeholders like `[P0]` are
  digits. It became a characterisation test (passes before and after the regex merge).
- The new `normalize_emotes` duplicated the prefix on a malformed tag. Its own test
  caught it before commit.
- The framing's joined buffer first resolved as "one root with an 18-byte header",
  which is the old last-message-only bug again. Fixed by ranking candidate framings
  (learned header size, then smaller header).
- The review's R4 note on `IS_SNIFFER_ACTIVE` was wrong: the reset re-emits "Active"
  to a reloaded UI. Not changed.

## Facts with a number
- Tests: core 31 → 78, types 0 → 3, UI 0 → 8.
- Warnings: app 14 → 0, UI 8 → 0.
- llama-server zip `v0.2.0/llama-b8157-bin-win-vulkan-x64.zip`: 28 057 281 bytes,
  SHA-256 `8144cf0a…20dc` (pinned in `downloader/server.rs`).
