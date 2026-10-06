# Structure improvement roadmap (2026-10 review)

Measured 2026-10-06 on `main` `1478528` (right after PR #203). Evidence, numbers and proposals are in [`docs/architecture_review.md`](../../docs/architecture_review.md)
(item IDs W / R / P / A / M, decisions D-1..D-7). Related: [`security_model.md`](../../docs/security_model.md), [`testing.md`](../../docs/testing.md), the decision log [`decisions.md`](../../docs/decisions.md). The earlier 2026-09 refactor is in [`refactor-2026-09.md`](refactor-2026-09.md); the real-app test automation roadmap is [`test-automation.md`](test-automation.md).

## Done-conditions of every stage

1. **Plan first.** Show an impact analysis (graft `callers` output) and wait for Kade's go (CLAUDE.md, workflow-control).
2. **Test first.** Write the failing unit test, see it fail for the right reason, then write the code. A bug fix starts with a reproducing test.
   Pure logic is tested in `crates/core` (or the ui's pure modules). Tauri / Windows glue that cannot be unit-tested is said so in the commit body.
3. **The gate of every part touched:** core / llama / types `just core-check`; ui `just ui-check`; app `just app-cross-check` (Linux, compile only; with and without `test-env`, and `--release --features test-env`) plus Windows CI `just app-check`. `cargo fmt` always.
4. **Refactors do not change behaviour.** The camelCase of `ChatMessage` / `SystemMessage` is the protocol; renaming a field is not a refactor. A new setting goes into both `AppConfig` types and the `app_config_full.json` fixture with the same name.
5. What could not be run goes into the last commit as `NOT VERIFIED: ...` (this environment cannot link or run the Windows app).
6. **One stage = one branch = one PR**; the next stage starts after the PR is merged. Update the *Now* section of `MEMORY.md` at the end.

## Decisions (2026-10-06, Kade)

- **D-1** Local and Beginner tab-limit default is **1000** (`default_tab_limits` 500 -> 1000 for those two; defined once, S2c).
- **D-2** `MEMORY.md` is limited to **40 lines and 6 KB** (the size is my recommendation, accepted) - S0a.
- **D-3** Smoke steps keep running after one fails, **but a release is blocked unless every step succeeds** - S0b.

## Next roadmap (2026-10-06 review)

Order: biggest risk first (losing the app or data) -> silent drops -> trust boundary -> structure -> efficiency. Every stage starts after Kade's go.

- **S0 Docs, memory and reading the real app** (M-1..M-4, W-13, D-2, D-3)
  - S0a Shrink `MEMORY.md` to 40 lines / 6 KB (now 287 lines / 103 KB), details to `.memory/`, plus a gate check that fails above the limit.
  - S0b `bridge-smoke.yml`: pipeline steps run with `if: always()`; the job still fails when any step fails, so `release.yml` (`needs: [check, build, smoke]`) still blocks. First check the pipelines are independent (each uses its own folder). CI change: own PR.
  - S0c Run `bridge-smoke.yml` by `workflow_dispatch` on `main` and read **`CS-restart-nodup`, `CP-big-ack` and the five skipped steps** (chat-rules `CR-ruby-*` and the 一人 reading K16, persistence `CP-fav-*`, translator-stub `TS-dict-*` and the K8 `<bos>` count, popups, download-integrity).
- **S1 Do not lose the app or data** (W-1, R-6, W-3, W-5)
  - S1a `install_swap` to core, rename back when the second rename fails. S1b extract the AI server into `.part`, then rename. S1c remove the export timestamp `unwrap`, add a panic hook and a log file.
- **S2 Stop silent drops** (W-4, W-7, W-6, R-4)
  - S2a archive on arrival. S2b settings: backup on read failure, `apply_config` returns a `Result`, `write_atomic` everywhere, `config_version`. S2c channel-limit defaults in `types` (**1000** for Local and Beginner). S2d `start_sniffer_command` / `ensure_firewall_rule_command` async, one firewall check.
- **S3 Trust boundary of the administrator app** (W-2, W-8, W-9)
  - S3a per-file SHA-256 check of the server exe and DLLs before spawn (or ACL). S3b minisign check of gist metadata, timeouts and size limits. S3c CSP, unused shell permissions removed, `open_browser` only `https`.
- **S4 Service-lifetime owner** (R-1, R-2, R-3, W-10) - `sniffer_change` table and watchdog decisions in core (fake clock), the 2-second duplicate window in core, then a `Services` owner.
- **S5 Small tidying and defects** (W-11, W-12, P-4..P-6, R-7..R-9, A-2.4) - `[P<n>]` collision (golden update), stray "model" decision, the six mutation gaps, batched block events, parser `unknown_fields`, block-list copies, `app_config.rs` / `types` split.
- **S6 UI tidying and efficiency** (P-1..P-3, R-5, A-4) - `display_limit` reset, per-row effects and IPC, big-view split. `ui-preview` screenshots and `cargo tauri dev` (Windows, administrator). Windowed list decided from the result.
- **S7 Sanitised capture corpus** (A-1.4) - Kade's real capture plus a substitution tool, replayed in CI.
- **Not doing:** merge the two `AppConfig`s (recorded decision), move the bridge / `test_env` out of core, a full tokio port, micro-optimising the parser and text path, UI click automation (Kade 2026-10-06). Reasons in review section 8.4.

## Finished in the 2026-10-06 session

- ~~**reqwest 0.11 -> 0.12**~~ **done** (PR #199): four advisories closed. Real app: translation, dictionary sync and favorites fine; the model download and the update check (no dialog) were not seen.
- ~~**Golden tests for the text pipeline**~~ **done** (PR #201): `crates/core/tests/golden_text.rs`, 8 snapshots; furigana left out until K16 is settled.
- ~~**Weekly mutation testing**~~ **done** (PR #202): `text.rs` 87 mutants: 75 caught, 8 missed, 4 unviable; 6 minutes on a hosted runner. The missed ones are test work in S5.
- ~~**Bridge MQTT packet limit**~~ **done** (PR #203): the cause of `CS-restart-nodup` (10 KiB default limit vs a 108 KB answer, then a reconnect). Limit 16 MiB, an over-limit answer becomes an error ack. The real-app result is read in S0c.
- ~~**Review and roadmap documents**~~ **done** (PR #204, Korean) and rewritten in English in this change.

## Numbers (2026-10-06)

| Part | Lines (`.rs`) | Note |
|---|---|---|
| `crates/core` | 13,843 | `kanji_on_table.rs` 2,982 (a table), `test_env.rs` 1,324, `bridge.rs` 1,102, `text.rs` 1,048 |
| `crates/types` | 1,261 | every DTO in one file (R-9) |
| `crates/llama` | 137 | |
| `src/` (ui) | 11,095 | `chat_view.rs` 908, `chat_row.rs` 597 (one function about 481 lines, reported) |
| `src-tauri/src` | 5,530 | `app_config.rs` 581, `sniffer/mod.rs` 450, `translator/mod.rs` 442 |
| `runbook/` (Python) | 8,501 | 11 pipelines |

Tests: 495 pass in core + llama + types; `#[test]` counts core 385 · types 49 · llama 61 · ui 157 · app 21. Golden tests 8. Workflows 9. `cargo audit` 0 vulnerabilities.

## Per-feature summary (details in review section 6)

| Feature | First thing to do | Stage |
|---|---|---|
| Capture and protocol | Error state on socket-setup failure; `Services` owner | S4 |
| Translation | `[P<n>]` collision fix; the six cache gaps from the mutation run | S5 |
| Chat storage | **archive on arrival** (a second line when a translation exists) | S2 |
| Overlay UI | `display_limit` reset; batch the furigana IPC | S6 |
| Settings, favorites, dictionary | backup on read failure; `config_version`; one definition of limits | S2 |
| Download, update, security | update rollback; server exe integrity; metadata signature | S1, S3 |
| Tests and QA | independent smoke steps; read the unread real-app rows; visual regression | S0, S6 |
| Operations | log file and panic hook; "copy diagnostics" | S1, later |

## Known defects (not fixed yet)

- A `[P0]` typed in chat collides with a real placeholder (`"[P0]火力"` -> `딜러딜러`). The golden `a_placeholder_typed_in_chat_collides_with_a_real_one_known_defect` pins today's behaviour. (S5)
- A leaked `<start_of_turn>model` leaves the word "model" (`model 번역`); what it should be is undecided. (S5)
