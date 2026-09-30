# Handoff: "do all recommended and opened" (2026-10-01)

Plan approved by Kade: three PRs, one at a time, each merged before the next starts
(CLAUDE.md, *Several tasks in one session*). Kade paused after task 2 and resumed task 3 the same day (CI is now ~4 min).

| # | branch | state |
|---|---|---|
| 1 | `claude/plain-history-frames` | **merged** (#67): plain `0x0003` `{3: channel, 5: chat}` is history (`framing.rs::decode`, `has_history_lines`) |
| 2 | `claude/beginner-channel` | **PR #68 open**, gates green locally; CI decides. `Channel::Beginner` (code 9), tab, colours, limit, switch cycle; golden parser pin re-recorded (proved: old hash returns when BEGINNER is printed as WORLD) |
| 3 | `claude/core-review-leftovers` | **PR open** -- W4, W6 (not `class_id`), W7, W8, W9; see the core-review note |

## Task 3 (next, after #68 is merged and its branch is gone)

Start from `git fetch origin main && git checkout -B claude/core-review-leftovers origin/main`.
Test first, one commit each, `just core-check` + `just app-cross-check`:

- **W4** dedup capacity is fixed at 4096: make it follow the chat limits (else limits above ~4096 re-emit reloaded lines). Look in `crates/core/src/capture/` (processor/dedup).
- **W6** parser gaps: `class_id` never set (sender tag 24 unknown -- only with a capture that decodes it; otherwise report, do not guess); `SenderInfo.is_blocked` never set; two channel-code tables disagree (`parser.rs` field-4 block maps only 3/4 and keeps the channel otherwise, `Channel::from_code` is the other); chunk type must precede its payload.
- **W7** IPv6 is ignored but the watchdog is fed first (`capture/pipeline.rs`, `feed_watchdog()` before the `NetHeaders::Ipv4` check).
- **W8** `pick_local_port` race, **W9** sniffer `Err(_) => continue` busy loop -- both `src-tauri/` (cross-check only on Linux; tests need Windows CI).

Details: [`core-review-2026-09-30.md`](../roadmap/core-review-2026-09-30.md), *Left open*.

## Not ours / waiting

- **A4** double `<bos>`: waits on Kade's re-fine-tune ([`review-2026-09-30-round2.md`](../roadmap/review-2026-09-30-round2.md)).
- Compact-mode hover: nobody has looked; needs a Windows run.
- Windows run needed for everything in [`unverified-on-windows.md`](../active-issues/unverified-on-windows.md), incl. the beginner tab.

## Session setup notes (Linux container)

`pip install rust-just`; `rustup target add wasm32-unknown-unknown x86_64-pc-windows-gnu`; `apt-get install gcc-mingw-w64-x86-64` -- then `just core-check`, `just ui-check`, `just app-cross-check` all run.
