# 2026-09-30 — translation ledger (catch-up at translator start)

Kade's ask: "translate with a ledger that ensures all messages are translated. The ledger
starts when the app opens and is updated on each successful translation. When the
translator starts, check the ledger, read chat_logs and translate from that point.
Only at translator start, no loop; afterwards just listen."

## Decisions (Kade, 2026-09-30)
- **Source: in-memory history, not chat_logs.** chat_logs are written only with
  `archive_chat` on (default off, and WORLD is in `archive_ignored_channels` by
  default), and a Japanese message reaches the log only after the translator handled
  it, so jobs queued while the model loads were never in it at start-up.
- **Newest 100 by default** (`translation_catch_up_limit`, both AppConfigs, default in
  `resonance_types::default_catch_up_limit`; 0 = off).

## Shape (my call, in the plan Kade approved)
- Ledger = set of owed pids (`workers::TranslationLedger`), not a watermark: with a
  watermark "advance on success", a failure followed by a success is skipped forever.
  System messages share the pid counter, so a contiguous pid watermark would never move.
- Recorded in `dispatch_pipeline_actions` for every Japanese message (translator on or
  off), settled in `process_translation_job` on success; failed/stale stay owed.
- Catch-up: drop the queue, take the newest N owed (older ones dropped from the ledger),
  translate oldest first from `ChatHistory::get`, live jobs first between items.
  `process_translation_job` skips pids no longer owed (dedups catch-up vs. queue).
- A message archived untranslated and later caught up is archived twice; `load_recent`
  now keeps only the newest line per (uid, timestamp, sequence_id). `dataset_raw.jsonl`
  keeps both lines.

Branch stacked on `claude/review-n11-n12-n13` (both change `load_recent`).
