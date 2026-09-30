# Core logic review — 2026-09-30

Findings from the review of `crates/core` (report was chat-only; this is the record).
One finding, one `claude/*` branch, one PR, test first (CLAUDE.md *TDD*).

| id | finding | status | done when |
|---|---|---|---|
| W3 | `ChatMessage` without `serde(default)`: old log lines stopped loading after a schema change | **done** #38 | old-format line parses |
| P1 | `load_recent` read every log ever written (retention 0 = forever) | **done** #39 | 60 d × 20k lines: 1591 → 54 ms; `MAX_SCAN_LINES` = 50k |
| W1 | stream key = server addr+port: two clients on one PC shared a frame assembler | **done** #40 | interleaved two-client test; idle streams dropped after 60 s |
| W5 | placeholder restore was a literal match (`[P 0]`, `[p0]`, `［P0］` lost the term) | **done** #41 | one tolerant regex pass |
| P2 | "dictionary cloned per job" | **dropped: wrong** | `AppState.dictionary` is already `RwLock<Arc<Dictionary>>` |
| P3 | `preprocess_text` 68 µs (2k terms) – 375 µs (10k) | **dropped** | < 0.1% of one LLM round trip; Aho-Corasick would change masking order (leftmost-longest vs longest-term-first) |
| R1 | one protobuf field iterator (`decoder::Fields`) instead of six hand loops | **done** (this branch) | parser output pinned by `parser_output_is_pinned_across_refactors` (40k generated packets, hash unchanged); `parser.rs` production code 425 → 282 lines |
| W2 | framing is guesswork (no TCP seq, app header not decoded) | **done** (`claude/festive-hypatia-5r78o7`) | length-based `FrameAssembler` (`protocol/framing.rs`), `ruzstd` for bit-15 frames (`protocol/compression.rs`, 1 MiB cap), TCP seq in `StreamTracker` (retransmits trimmed, gap resets), `parser::history_pipeline`. Real capture: 256 packets -> 160 chats, matching an independent python decode |

## W2 — what the first real capture showed (2026-10-01, 256 packets, 87 KB, no loss or retransmits)

Frame = `[u32 BE total length incl. header][u16 type][body]`. It splits exactly on the lengths.

| type | n | body | handled |
|---|---|---|---|
| `0x0002` | 40 | 16-byte header + root `{1: channel, 2: chat}` -- live chat (33) + a few non-chat roots | live |
| `0x0003` | 21 | 12-byte header + root `{3: chat}` -- the player's **own** line echoed, always also sent as `0x0002` | skipped |
| `0x0004` | 129 | none (6 bytes) -- keepalive | skipped |
| `0x8003` | 11 | 12-byte header + zstd(root `{3: channel, 5: chat x ~30}`) -- channel history, newest first, re-sent now and then | history (oldest first) |
| `0x8002` | 15 | 16-byte header + zstd -- big blob (urls, player data), not chat | skipped |

Chat = `{1: id, 2: sender{1: uid, 2: name, ..}, 3: time, 4: message{3: text | 1: rich}}` in all of them.
The 33 live lines were already all shown before W2; the win is the 127 history lines the
client had not sent live (141 unique in history, 14 also seen live). What is not known:
- Channel codes seen in live frames: 1, 2, 3, 4, 9 (and a bytes-typed field 1). The parser maps 2 LOCAL, 3 PARTY, 4 GUILD, everything else (1, **9**) WORLD -- 9 (the beginner channel, per Kade's test lines) is shown as WORLD. History carries the code in root field 3.
- History lines from **other channels/lines** (ids in other ranges: 1205xxx, 6952) are shown too, as WORLD; each Japanese one is queued for translation (a refresh brings up to ~30). If that floods the translator, decide which history to keep.
- What the 16/12-byte inner headers hold (`0x0002` header: 4 zero bytes, a u32 id, ...) and `0x8002`'s content.
- The `0x0003` skip assumes it never carries a line `0x0002` lacks (true for all 7 here).

## Left open (low, no branch yet)

- W4 dedup capacity (4096) is fixed; limits above ~4096 in total re-emit old reloaded lines.
- W6 parser gaps: `class_id` never set (sender tag 24 is unknown), `SenderInfo.is_blocked`
  never set, two channel-code tables disagree, chunk type must precede its payload.
- W7 IPv6 ignored but the watchdog is fed first. W8 `pick_local_port` race. W9 sniffer
  `Err(_) => continue` busy loop. Channel as an enum in `resonance-types`.
