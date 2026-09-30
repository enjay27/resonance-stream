# Review round 2 — 2026-09-30 (core + UI communication + LLM server)

Findings of the second review (report was chat-only; this is the record). Kade approved
the order below ("proceed by your order"). One step, one `claude/*` branch, one PR, test first.

| id | finding | status |
|---|---|---|
| A1 | service states only as events: a state emitted before the UI listened (translator starts with the app; page reload) was lost | **done** `claude/service-state-query` — `ServiceStates` (types) with `seq`, `get_service_states` command, UI `SeqGate` |
| A2 | llama-server crash after start not noticed: jobs fail silently, badge stays "Active", no restart | **done** `claude/translator-supervisor` — `workers::ServerSupervisor` (restart on exit / 3 failures in a row, backoff 2-4-8 s, give up after 3 in 10 min); worker checks the child every 5 s idle |
| A9 | health check ignores a dead child (waits 30 s); 30 s fixed | **done** (step 2) — `wait_for_server` stops at once with the exit status; limit `SERVER_START_TIMEOUT` 90 s |
| A3 | hydration replaces the chat store: events during `get_chat_history` are lost | **done** `claude/hydration-merge` — `ChatStore::merge_history` (pid order, live copy wins). Left: a translation event for a pre-listen row that lands during the fetch is still lost (small window) |
| A5 | two readiness checks overwrite `model_ready` (model missing + server present = "starting") | **done** (step 3) — `hydration::translator_ready` |
| A6 | `RwSignal::new` per chat row outside an owner: evicted rows never freed | **done** `claude/chat-signal-leak` — confirmed in reactive_graph 0.2.15 `ArenaItem::new_with_storage` (no owner = never disposed) and by test (0 of 2 freed); store holds `ArcRwSignal`, rows wrap it in an owned `RwSignal`. System log likewise |
| A4 | prompt starts with literal `<bos>`; llama-server adds BOS too (double BOS) | **blocked: needs Kade** — changing it changes what the fine-tuned model sees. Question: did `make_prompt()` training tokenize with the tokenizer adding BOS (then training had 2 as well: keep) or not (then drop `<bos>` from `translation_prompt`)? The llama-server log (B3) also shows the "2 BOS tokens" warning. Prompt now pinned in core |
| A8 | `max_tokens: 512` for every line; runaway output blocks the queue | **done** `claude/prompt-in-core` — `text::output_token_limit` (3/char + 32, 64..512), sent as `n_predict` and `max_tokens`. C1 done in the same branch (prompt, sanitising, `contains_japanese` moved to core, pinned) |
| B1 | translation cache (LRU on masked text) | open (step 6) |
| A7 | stale/failed job archived untranslated, then again translated after catch-up | open (step 7) |
| A10 | `app_data_dir().unwrap()` in `launch_ai_server` | **done** in step 2 (same function) |
| B3 B4 B6 C2 C3 | server stderr log; `-t` from CPU count; `contains_japanese` ranges; enums for states/tabs; `AppSignals` grouping | open (step 7) |

C4 (translator supervisor) moved from step 1 into step 2: it is the crash-handling part.
