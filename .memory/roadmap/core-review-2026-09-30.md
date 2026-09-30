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
| W2 | framing is guesswork (no TCP seq, app header not decoded; compressed frames would be dropped silently). **Integration tests (2026-09-30) found real loss around rich lines** -- see `sessions/2026-09-30-integration-tests.md`; pinned in `crates/llama/tests/capture.rs` | **blocked: needs a real capture** | Kade captures port-5003 traffic at home; then decode the header, split by length, drop retransmits by `tcp.sequence_number` |

## W2 — what the capture should contain

A `.pcap` (Wireshark/`dumpcap`, filter `tcp port 5003`, both directions) with: one short
chat line; one **long** message; a **burst** of several messages in a few seconds; ideally a
message with an item link. Note the wall-clock time of each message sent so frames can be
matched. Open questions it answers: is the app header length-prefixed, is there a
compression flag (hypothesis, unverified), are there frames the current framing drops.

## Left open (low, no branch yet)

- W4 dedup capacity (4096) is fixed; limits above ~4096 in total re-emit old reloaded lines.
- W6 parser gaps: `class_id` never set (sender tag 24 is unknown), `SenderInfo.is_blocked`
  never set, two channel-code tables disagree, chunk type must precede its payload.
- W7 IPv6 ignored but the watchdog is fed first. W8 `pick_local_port` race. W9 sniffer
  `Err(_) => continue` busy loop. Channel as an enum in `resonance-types`.
