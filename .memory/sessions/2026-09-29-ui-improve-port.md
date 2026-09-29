# 2026-09-29 — porting feat/ui-improve onto main

`feat/ui-improve` (10 commits, Mar 9 – Apr 4) forked at 33f6cee, before the refactor
(PR #23) and the review fixes (PR #26). A trial merge conflicted in 15 files, 4 of them
modify/delete (files split or moved into `crates/core`). Instead of one merge, each commit
was replayed onto main, adapted, with Kade as author and the adaptation in the message.

## Kade's decisions
1. Pids stay sequential (the branch hashed (uid, ts, seq): scrambled the pid-ordered
   list and made every "Me" message collide). Restart dedup comes from seeding the
   sniffer's duplicate check with the reloaded history instead.
2. Keep both archives: `dataset_raw.jsonl` and daily `chat_logs/*.jsonl`.
3. The gist publishes `sha256` → downloads require it (empty = refused).
4. Dictionary auto-sync off by default; the gate is the silent start-up sync only.

## Mapping
| branch | on main |
|---|---|
| 57042af firewall check | a5b1966 (+ fixes main's `sync_dictionary` calls missing `version`) |
| 8ca83e6 model hash | 1596499 (hash while streaming; no dead verify command) |
| 7389fb3 firewall reset | ae4a597 (restores `remove_firewall_rule`, one delete not two) |
| 9937ef2 mock test | skipped — main already had the same fix |
| b911ae7 history (backend) | efb1839 |
| b911ae7 (UI) + 0ce143e + 3bc6e28 | 761617b per-tab views/limits, one commit |
| ec8a19b spacing | adc4f6c |
| 43b4530 window state | 7f931fb |
| 00a7068 auto-sync default | 24fa5b9 (+ the gate, UI default) |

## Wrong turns
- First read the all-tab limit as user-editable ("전체" limit input exists in the menu
  markup); the tab has no context menu at all. Reverted to the branch's later rule:
  channel limits summed. Test pinned by mutation.
- The branch's GC (`retain` over every tab's list per message) was quadratic; replaced
  by per-pid view counts (`TabViews`).

## Open
- `check_dict_update` is invoked by hydration but no such command is registered (it
  predates this port; the call just errors). Not changed.
