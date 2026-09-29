# Code Review — Inefficiencies, Bugs and Enhancements

*resonance-stream @ `1b21f8d` · 2026-09-29 · analysis only, no code changed*

## How this was produced

- Read every file on the hot path end to end: capture → protocol → dedup → events →
  translator → UI list rendering, plus config, downloader, updater and manifests.
- **Every claim marked ✅ was reproduced**: with a throw-away probe crate that links
  `resonance-core` and feeds it synthetic packets (Appendix A), or with a cargo
  warning. Claims marked 🔍 come from reading the code and were not run. The app crate
  (`src-tauri/`) cannot run on Linux, so nothing there was executed.
- Gates this session: `just fmt-check` ✅, `just core-check` ✅ (31 tests pass).
  UI and app gates not run (no code changed).

## Summary

| # | Sev | Part | Finding | Effort |
|---|---|---|---|---|
| B1 | **High** | core | Crafted length varint panics the parser (20-byte packet) ✅ | S |
| B2 | **High** | core | Coalesced chat packets lose all but the last one; split packets are dropped entirely ✅ | M |
| B3 | **High** | core | Only the **first** "Me" (field-4) message is ever shown; later ones are dropped as duplicates ✅ | S |
| P1 | **High** | app | `config.json` is read from disk and parsed **for every IP packet** the raw socket sees 🔍 | S |
| P3 | **High** | app | Translator holds `nickname_cache` lock during the HTTP call, stalling the sniffer 🔍 | S |
| B4 | **High** | app | Turning translation on starts **two** translator workers / llama servers 🔍 | S |
| B5 | Med | build | `[profile.release]` (LTO, strip, `opt-level="s"`, `panic="abort"`) is ignored ✅ | S |
| B6 | Med | app | Dictionary sync/edit says "reloaded" but the translator keeps its startup copy 🔍 | S |
| B7 | Med | app | Model re-download deletes the old model first; a failed download leaves none 🔍 | S |
| P2 | Med | app | `blocked_users` HashMap cloned for every IP packet 🔍 | S |
| P4 | Med | core | Dedup cache grows without bound for the whole session 🔍 | S |
| P5 | Med | app+ui | `IndexMap::shift_remove_index(0)` is O(n) on every message once the history is full 🔍 | S |
| P6 | Med | core | `postprocess_text` compiles 2 regexes per translation; 4 number regexes run as 4 passes 🔍 | S |
| P7 | Med | core | `preprocess_text` sorts the whole dictionary and nickname cache on every message 🔍 | M |
| P8 | Med | ui | Chat filter memo clones every `ChatMessage` 1–3× per new message and re-runs on every translation 🔍 | S |
| P9 | Low | ui | Every Tauri event is deserialized twice (JS → `serde_json::Value` → struct) 🔍 | S |
| P10 | Low | types | `unknown_fields` (raw bytes) serialized to the UI and cloned with every message 🔍 | S |
| P11 | Low | core | `PacketBuffer` drains from the front of a `Vec` (O(n) shifts, byte-at-a-time resync) 🔍 | S |
| P12 | Low | app | Download progress emitted per network chunk (thousands of IPC events) 🔍 | S |
| P13 | Low | app | Translation queue is unbounded and strictly serial; bursts create minute-long lag 🔍 | M |
| P14 | Low | app | Raw socket uses the default receive buffer 🔍 | S |
| R1–R8 | Low–Med | app/ui | Robustness and design (main-thread sleep, `taskkill` scope, archive path, updater, …) 🔍 | — |
| D1 | Low | all | Dead state, duplicate code, 8 unused dependencies 🔍 | S |

Effort: **S** = under an hour, local change; **M** = a few hours, needs tests.

---

## 1 · Correctness bugs

### B1 — Length arithmetic overflows on untrusted input ✅ (High, core)

Every length-delimited field does `(pos + len as usize).min(end)` with `len` a raw
`u64` from the wire:

- `crates/core/src/protocol/parser.rs:66, 79, 196, 212, 246, 282, 328, 374, 385, 429`
- `crates/core/src/protocol/decoder.rs:36, 51`
- `crates/core/src/protocol/packet_buffer.rs:59`

With a 10-byte varint near `u64::MAX` the addition overflows. The probe fed a
**20-byte** packet to `parsing_pipeline`:

```
debug:   panicked at parser.rs:385:29: attempt to add with overflow
release: panicked at parser.rs:386:38: slice index starts at 11 but ends at 10
```

In release the sum wraps, `r_end < j`, and `&data[j..r_end]` panics. The panic
unwinds and kills the sniffer thread without a message. The watchdog then reports "no
traffic" 15 s later, and the user sees a red badge with no cause. If B5 is fixed
as written (`panic = "abort"`), the same packet would **terminate the whole app**.
`PacketBuffer` resyncs on any `0x0A` in the byte stream, so garbage reaches these
decoders routinely. A huge varint is rare, not impossible.

**Fix.** Use one helper everywhere:

```rust
/// End of a length-delimited field, clamped to `limit`; never overflows.
fn field_end(start: usize, len: u64, limit: usize) -> usize {
    usize::try_from(len).ok().and_then(|l| start.checked_add(l)).map_or(limit, |e| e.min(limit))
}
```

Also make `skip_field` return a saturated value. Add a property test that feeds random
bytes to `parsing_pipeline` and asserts no panic (e.g. `proptest`, or a loop over a
seeded RNG, to keep dependencies small).

### B2 — Reassembly is unreachable; coalesced messages are lost ✅ (High, core)

`strip_application_header(…, 5003)` (`parser.rs:423-435`) accepts a TCP segment only
if some `0x0A`+varint length ends **exactly at the end of the segment**. So:

| input | expected | got |
|---|---|---|
| 1 message per segment | Hello, World | Hello, World |
| 2 messages in 1 segment | Hello, World | **World** |
| 1 message split over 2 segments | Hello | **(nothing)** |

The first segment of a split message never ends on a boundary, so it is discarded
before it reaches `PacketBuffer`. The reassembly and watchdog code in
`packet_buffer.rs` is effectively dead. When several messages share a segment, the
scan skips past the earlier ones and returns only the last.

**Caveat.** The probe uses the framing that the repo's own `test_full_chat_pipeline`
assumes (4-byte header, then protobuf). If the real game framing is a
length-prefixed frame, parse *that* header (size + type) and feed the frame body to
`PacketBuffer`. Guessing the start from `0x0A` is fragile either way. Next step:
capture a real burst (e.g. a busy world chat) and add it as a fixture test.

**Fix direction.** Push the whole TCP payload into the per-stream buffer. Let
`PacketBuffer` find frame boundaries, using the application frame header if one exists.
Track the TCP sequence number so retransmits and out-of-order segments do not corrupt
the stream.

### B3 — "Me" messages collapse to one ✅ (High, core)

Field-4 blocks (`parser.rs:146-157`) set only `message` and `channel`, so `uid`,
`timestamp` and `sequence_id` all stay `0`. The dedup key in
`message_processor.rs:32` is `(uid, timestamp, sequence_id)`, which is `(0,0,0)` for
**every** such message. After the first one, all are `IgnoreDuplicate`. Probe: two
different "Me" messages → only `Me:one` emitted.

**Fix.** Skip the processor dedup when the signature is all zero. Or include a hash of
the message text in the key, as `events.rs::generate_message_fingerprint` already does.

### B4 — Enabling translation starts two workers 🔍 (High, app)

`save_config` (`src-tauri/src/config/app_config.rs`) handles the translation toggle
**twice**: lines 150-165 and again lines 189-222. On *off → on* both blocks call
`start_translator_worker`. Each worker runs `taskkill /IM llama-server.exe` and then
spawns its own server on port 8080. They race: the second `taskkill` can kill the
first server mid-load, and two model loads compete for VRAM. On *on → off*, the
"AI Translation Disabled" system message is logged twice.

**Fix.** Delete lines 149-165. The second block already covers on, off and spec changes.

### B5 — Release profile is silently ignored ✅ (Med, build)

`src-tauri/Cargo.toml:57` defines `[profile.release]`, but `src-tauri` is not the
workspace root. Every cargo invocation prints:

```
warning: profiles for the non root package will be ignored, specify profiles at the workspace root
```

So shipped builds get no LTO, no strip, `opt-level = 3` and `panic = "unwind"`. That
means a bigger installer than intended, and the size choices were never applied.

**Fix.** Move the block to the root `Cargo.toml`. Fix **B1 first**: with
`panic = "abort"` any parser panic ends the process. Consider `opt-level = 3` for
`resonance-core` via `[profile.release.package.resonance-core]`, since it is the hot
loop.

### B6 — Dictionary updates never reach the translator 🔍 (Med, app)

`start_translator_worker` loads `custom_dict.json` once (`translator/mod.rs:58-59`) and
keeps it on the worker's stack. `sync_dictionary` (`downloader/gist.rs:60-109`)
returns `"Dictionary updated and reloaded!"`, and `save_local_dictionary` writes the
file, but neither affects the running translator. The new terms apply only after
translation is toggled or the app is restarted.

**Fix.** Keep the dictionary in `AppState` as `RwLock<Arc<Dict>>` and swap it after a
sync or save. The worker clones the `Arc` per job. This also fits P7: build the
sorted list or automaton once, at swap time.

### B7 — Model update is destructive before it succeeds 🔍 (Med, app)

`download_model` (`downloader/model.rs:71-75`) runs `remove_dir_all` on the model
folder **before** the download starts. A network drop, a 404 or a full disk leaves the
user with no model and a broken translator. No size or checksum is verified.

**Fix.** Download to `model.gguf.part`, check `content_length` (and a SHA-256 from the
gist metadata), then `rename` over the old file. The same pattern applies to
`server.rs` (zip) and `app_updater.rs`.

---

## 2 · Performance

Context: `SIO_RCVALL` delivers **every IPv4 packet on the interface** to the sniffer
loop, not just game traffic. Browser, Discord, a stream upload and the game itself
can mean thousands of packets per second. Anything done per `recv` is multiplied by
that rate.

### P1 — Disk read + JSON parse per captured packet 🔍 (High, app)

`sniffer/mod.rs:146` calls `dispatch_pipeline_actions` after **every** `recv`, even
when the pipeline returned no actions. Its first line (`:199`) is
`load_config(app.clone())`, which does `config_dir.exists()` (+ `create_dir_all`) →
`path.exists()` → `read_to_string` → `serde_json::from_str` (`app_config.rs:88-119`). `store_and_emit`
does it again per chat message (`events.rs:116`), as do `block_user_command` and
`unblock_user_command`.

**Fix (two steps):**
1. Return early: `if actions.is_empty() { continue; }` before dispatching. This is a
   one-line change and removes almost all of the cost.
2. Keep the live config in `AppState` (`RwLock<AppConfig>`), filled at start-up and
   replaced in `save_config`. The `#[tauri::command] load_config` then reads memory.
   Add `AppState.config` in `src-tauri` only; the UI's `AppConfig` does not change
   (the two-type split stays).

### P2 — `blocked_users` cloned per packet 🔍 (Med, app)

`sniffer/mod.rs:127-128` locks the mutex and **clones the whole HashMap** for every
received packet before the port check. **Fix:** pass a closure
`|uid| blocked.lock().unwrap().contains_key(&uid)` into the pipeline, so the lock is
taken only per chat message. Or store `RwLock<Arc<HashMap<…>>>` and clone the `Arc`.

### P3 — Lock held across the HTTP translation 🔍 (High, app)

`process_translation_job` (`translator/mod.rs:86`) takes
`state.nickname_cache.lock()` and keeps it through `translate_text`, a blocking HTTP
call that takes hundreds of milliseconds to seconds on CPU. Meanwhile the sniffer
thread needs the same lock for each Japanese nickname (`sniffer/mod.rs:215`) and in
`store_and_emit` (`events.rs:108`). **The sniffer blocks for the whole translation.**
The kernel raw socket buffer (P14) then fills and packets, including chat, are dropped.

**Fix.** Scope the lock to preprocessing:
```rust
let shield = {
    let nick_cache = state.nickname_cache.lock().unwrap();
    preprocess_text(&chat.message, dict, Some(&nick_cache))
};
let raw = translate_text(client, AI_SERVER_URL, &shield.masked_text);
```

### P4 — Unbounded dedup cache 🔍 (Med, core)

`MessageProcessor::dedup_cache` (`message_processor.rs:11, 48`) gains one entry per
message and is never pruned. It is small per entry, but a long streaming session grows
it without limit. There is also a second, time-windowed dedup in `events.rs:15, 79-103`
(`CHAT_DEDUPE_CACHE`, 2 s, linear scan). **Fix:** keep at most the last N signatures
(`VecDeque` + `HashMap`, N ≈ `chat_limit`), or evict by age. Then decide whether both
dedup layers are still needed.

### P5 — O(n) eviction from `IndexMap` 🔍 (Med, app + ui)

`history.shift_remove_index(0)` (`events.rs:119-121`, `hooks/use_events.rs:78-80`)
shifts every remaining entry. Once the log is full (default 1000), every new message
costs a 1000-element shift on both sides of the boundary. **Fix:** chat pids are
monotonic, so a `VecDeque<ChatMessage>` with a binary search by pid for updates (or a
`HashMap<pid, index>` offset by a head counter) gives O(1) eviction.
`IndexMap::drain(..k)` in batches of e.g. 10 % also works with minimal change.

### P6 — Regex compiled per translation 🔍 (Med, core)

`text.rs:149` and `text.rs:153` call `Regex::new` inside `postprocess_text`, on every
message. Compilation costs far more than matching. Move both into the existing
`lazy_static!` block. The four `NUM_PATTERN_*` passes (`text.rs:85-119`) each rebuild
the string. Merge them into one `(\d+)(種|人|周|回)` with a lookup table: one pass,
one allocation.

### P7 — Dictionary sorted per message 🔍 (Med, core)

`preprocess_text` sorts the whole custom dictionary (`text.rs:75-76`) and the whole
nickname cache (`:54-55`) by length on every message. It then runs `contains` for each
entry, which is O(D·L). The nickname cache grows with every Japanese name seen in the
session. **Fix:** sort once when the dictionary is loaded (B6), or better, build one
`aho-corasick` automaton with `MatchKind::LeftmostLongest` over dictionary + names and
replace in a single pass. That also avoids a latent bug where a later term matches
inside an earlier `[P3]` placeholder (e.g. a dictionary key `P3` or `3`).

### P8 — UI filter memo clones the whole log 🔍 (Med, ui)

`components/chat_container.rs:29-105` calls `m.get()` in each filter stage. That
**clones the full `ChatMessage`** (including `unknown_fields`) for every message in the
log, up to three times, on every new message. It also subscribes the memo to every
row's signal, so each `translation-event` re-filters the whole log. It then builds the
full filtered `Vec` just to keep the last `display_limit` items.

**Fix.** Use `m.with(|m| …)` (no clone) and iterate from the back with `.rev()` →
filter → `.take(limit)` → reverse. Track row signals only for fields that affect
filtering: channel and level are immutable, so read them with `with_untracked`. The
same `get()`-clones-everything pattern appears in `chat_row.rs` (`:35, 45, 120, 196,
272, 335`); `sig.with(|m| m.channel.clone())` is enough for those.

### P9 — Double deserialization of every event 🔍 (Low, ui)

`hooks/use_events.rs:42-43` (and each handler) converts JS → `serde_json::Value` →
clones `ev["payload"]` → struct. **Fix:** deserialize once into
`#[derive(Deserialize)] struct Event<T> { payload: T }` with `serde_wasm_bindgen`.

### P10 — `unknown_fields` crosses the IPC boundary 🔍 (Low, types)

Every `ChatMessage` carries `HashMap<String, Vec<u8>>` of unparsed fields. Each entry
has a `format!`-allocated key and a `to_vec` copy (`parser.rs:224, 255, 294, 353,
400`). The map is serialized to JSON as number arrays, stored in history, and
deserialized in wasm. Nothing in the UI reads it. **Fix:** collect it only in debug
mode, or `#[serde(skip_serializing_if = "HashMap::is_empty")]` and leave it empty in
release. This is a wire-format change: the field keeps its name, only its contents
change.

### P11 — `PacketBuffer` front-drains a `Vec` 🔍 (Low, core)

`packet_buffer.rs:50, 66, 72` call `drain(0..n)`, which shifts the whole remaining
buffer. The fake-`0x0A` path drains **one byte per iteration**, O(n²) on a garbage
buffer. **Fix:** keep a read offset and compact only when it passes half the capacity,
or use `bytes::BytesMut::advance`. Also swap `SystemTime` for `Instant` in the
watchdog; wall-clock jumps (NTP, sleep) can trigger spurious resets. Mostly moot until
B2 is fixed, since the buffer is rarely used today.

### P12 — Progress event per chunk 🔍 (Low, app)

`downloader/model.rs:99-114` (and `server.rs:76`, `app_updater.rs:47`) emit
`download-progress` for every chunk, often 8–16 KiB. A multi-GB model means
hundreds of thousands of IPC events and UI signal updates. **Fix:** emit only when `percent`
changes.

### P13 — Translation backlog 🔍 (Low, app)

The translator channel is `unbounded()` and processed one job at a time with
`--parallel 1`. In a busy world chat, messages arrive faster than a CPU model
translates. The queue grows and translations show up minutes late for rows that have
already scrolled away. **Options:** a bounded channel (`try_send`, drop oldest), skip
jobs older than N seconds, prioritise PARTY/GUILD over WORLD, or run `--parallel 2-4`
with a small worker pool (llama.cpp batches concurrent slots well on GPU).

### P14 — Default raw-socket receive buffer 🔍 (Low, app)

`setup_raw_socket` (`sniffer/network.rs:99`) keeps the OS default `SO_RCVBUF`.
Together with P1 and P3, bursts overflow it. Add `socket.set_recv_buffer_size(4 << 20)`.

---

## 3 · Robustness and design

- **R1 · Blocking sync commands on the main thread.** In Tauri 2, a command without
  `async` runs on the main (UI) thread. `restart_sniffer_command` sleeps 500 ms
  (`sniffer/mod.rs:306`). `ai_server_health_check` makes a blocking HTTP call.
  `save_config` does disk I/O and thread orchestration. Make them `async` or
  `#[tauri::command(async)]`.
- **R2 · `taskkill /F /IM llama-server.exe` kills every llama server on the machine**
  (`server_manager.rs:141-154`), including a user's own, unrelated ones. Port 8080 is
  hard-coded (`:33`, `core.rs:4`) and is also a common dev port. Track the child PID (it
  is already in `ServerGuard`), kill only that, and pick a free port at launch.
- **R3 · Mutex poisoning cascade.** Every lock is `.lock().unwrap()`. One panic while a
  lock is held (e.g. in the translator) poisons it, and every later command that
  touches it panics too. Use `parking_lot::Mutex` (no poisoning) or
  `.lock().unwrap_or_else(|e| e.into_inner())`.
- **R4 · Start-up ordering.** `lib.rs:69-84` starts the translator and archive workers
  **before** `app.manage(AppState)` (`:89`). `inject_system_message` uses
  `try_state`, so the worker's first log lines can be dropped silently.
  `start_sniffer_command` also resets `IS_SNIFFER_ACTIVE` to `false` when it refuses
  to restart (`sniffer/mod.rs:54`), which looks unintended. Manage the state first,
  then spawn.
- **R5 · Archive location and cost.** `io/data_factory.rs:44, 75` append
  `"../../../dataset_raw.jsonl"` to the AppData dir. That resolves to the user's
  **home folder**, not the app folder. The file is also reopened for every message.
  Keep a `BufWriter<File>` open in the worker and flush on a timer. `save_to_data_factory`
  is an unused copy of `append_to_file`.
- **R6 · Self-update without verification.** `download_app_update` accepts any URL from
  the frontend and replaces the running exe (`app_updater.rs`) without a hash or
  signature check, in a process that runs **as Administrator**. Anyone who can edit
  the gist can ship a binary. Consider `tauri-plugin-updater` (minisign-signed), or at
  least a SHA-256 in the gist plus a host allow-list.
- **R7 · Sticker/sprite normalisation is in the wrong place, twice.** The
  `emojiPic=` / `<sprite=…>` rewrite is copy-pasted in `hooks/use_events.rs:45-74` and
  `app/hydration.rs:116-147`, and the copies differ (only the live one resets
  `translated`). It runs in the UI **after** the backend has already sent the raw text,
  sprite tags included, to the LLM, which wastes a translation and can garble the output.
  Move it to `crates/core/src/text.rs` (testable) and apply it in the pipeline before
  translation.
- **R8 · Duplicate code.** `ensure_firewall_rule` and `ensure_firewall_rule_command`
  (`sniffer/network.rs:194-255` vs `288-352`) are the same netsh sequence. Make it one
  function that returns `Result`, with the command as a thin wrapper.
- **R9 · Decoder only handles 1-byte tags.** `decoder.rs` and the parsers match on a
  single tag byte. Fields numbered 16 and up (2-byte tags) are misread as other fields.
  This is fine for today's fields, but it is a trap when the protocol grows. Decode the
  tag as a varint.
- **R10 · Version check is string inequality** (`gist.rs:32, 41`). A stale gist with
  an *older* version prompts a "downgrade update". Compare with `semver`.

## 4 · Dead code and unused dependencies 🔍

- **State never read:** `AppState.batch_data` + `MessageRequest`
  (`protocol/types.rs:14, 37`) and `AppState.dedup_cache` (`:22`; the real one lives in
  `MessageProcessor`). `save_to_data_factory` (`io/data_factory.rs:27`).
- **Unused imports** in `sniffer/mod.rs:8, 20` (`Hash`, `Hasher`, `CommandExt`), and an
  unused `e` in `ai_server_health_check`. rustc warns on these, and the cross-check
  already reports 19 warnings.
- **Dependencies with no references in source** (grep; confirm with `cargo machete`):
  `src-tauri`: `byteorder`, `aes`, `ctr`, `hex`, `tokio`, `tauri-plugin-opener`
  (the plugin is never registered), dev `pnet`. UI: `tailwind-css` (a Rust crate;
  Tailwind runs through the npm CLI). Removing them cuts cold build time and the
  Windows CI cache.
- `reqwest 0.11` → `0.12` when convenient (hyper 1.x, fewer duplicate deps with Tauri).

---

## 5 · Suggested order

Each step is one commit and one gate, per `CLAUDE.md`.

| step | items | part → gate | why first |
|---|---|---|---|
| 1 | B1, B3, P6 + fuzz test | core → `just core-check` | crash + data loss, pure logic, fully testable on Linux |
| 2 | P1 (early return + cached config), P2, P3 | app → Windows CI | biggest runtime win, small diffs |
| 3 | B4, B5 (after step 1), B7 | app / build → Windows CI | one-block deletions and moves |
| 4 | B2 with a real captured fixture | core → `just core-check` | needs a real capture to confirm framing |
| 5 | B6 + P7 (shared dictionary, Aho-Corasick) | core + app | a design change, touches both sides |
| 6 | P5, P8, P9, R7 | ui (+core for R7) → `just ui-check` + manual run | UI smoothness at `chat_limit` 1000 |
| 7 | R1–R6, D1, remaining Low items | mixed | hygiene |

Steps 2, 3 and 5 touch `src-tauri/`, so a Linux session can only cross-check them.
They need the Windows CI job and a manual `cargo tauri dev` run as Administrator.

---

## Appendix A — Probe used for ✅ items

A standalone crate outside the repo, depending on `resonance-core` by path:

```rust
// B1: 20-byte packet, message sub-tag 58 with a u64::MAX length varint
let huge = [0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x01];
let mut msg = vec![58u8]; msg.extend_from_slice(&huge); msg.extend_from_slice(&[0,0,0]);
let mut chat = vec![34u8, msg.len() as u8]; chat.extend(&msg);
let mut root = vec![0x12u8, chat.len() as u8]; root.extend(&chat);
let mut pkt  = vec![0x0Au8, root.len() as u8]; pkt.extend(&root);
parsing_pipeline(&pkt); // panics (debug: add overflow; release: slice index)

// B2/B3: IPv4/TCP from port 5003 via etherparse::PacketBuilder,
// payload = [0,0,0,0] ++ protobuf root, fed to ChatPipeline::feed_network_packet
baseline: 1 msg per segment      -> ["Bob:Hello", "Bob:World"]
2 msgs coalesced in 1 segment    -> ["Bob:World"]
1 msg split across 2 segments    -> []
two different 'Me' messages      -> ["Me:one"]
```

These are ready to become regression tests in `crates/core` once each fix lands.
