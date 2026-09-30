# Review round 2 — 2026-09-30 (core + UI communication + LLM server)

Findings of the second review (report was chat-only; this is the record). Kade approved
the order below ("proceed by your order"). One step, one `claude/*` branch, one PR, test first.

| id | finding | status |
|---|---|---|
| A1 | service states only as events: a state emitted before the UI listened (translator starts with the app; page reload) was lost | **done** `claude/service-state-query` — `ServiceStates` (types) with `seq`, `get_service_states` command, UI `SeqGate` |
| A2 | llama-server crash after start not noticed: jobs fail silently, badge stays "Active", no restart | **done** `claude/translator-supervisor` — `workers::ServerSupervisor` (restart on exit / 3 failures in a row, backoff 2-4-8 s, give up after 3 in 10 min); worker checks the child every 5 s idle |
| A9 | health check ignores a dead child (waits 30 s); 30 s fixed | **done** (step 2) — `wait_for_server` stops at once with the exit status; limit `SERVER_START_TIMEOUT` 90 s |
| A3 | hydration replaces the chat store: events during `get_chat_history` are lost | open (step 3) |
| A5 | two readiness checks overwrite `model_ready` (model missing + server present = "starting") | open (step 3) |
| A6 | `RwSignal::new` per chat row outside an owner: evicted rows likely never freed | open (step 4, verify first) |
| A4 | prompt starts with literal `<bos>`; llama-server adds BOS too (double BOS) | open (step 5, confirm in server log on Windows) |
| A8 | `max_tokens: 512` for every line; runaway output blocks the queue | open (step 5) |
| B1 | translation cache (LRU on masked text) | open (step 6) |
| A7 | stale/failed job archived untranslated, then again translated after catch-up | open (step 7) |
| A10 | `app_data_dir().unwrap()` in `launch_ai_server` | **done** in step 2 (same function) |
| B3 B4 B6 C2 C3 | server stderr log; `-t` from CPU count; `contains_japanese` ranges; enums for states/tabs; `AppSignals` grouping | open (step 7) |

C4 (translator supervisor) moved from step 1 into step 2: it is the crash-handling part.
