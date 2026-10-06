# Decisions

A short log of decisions that shape the project: what was decided, by whom, when, why, and what follows. It replaces scattered "Kade decided" notes in `MEMORY.md` and session files.
Newest first within each group. **Add an entry when a decision is made; change the status when it is reversed, never delete it.**

Sources: a quote or a note in `CLAUDE.md` / `MEMORY.md` is cited as such. "Claude" means a decision taken by the assistant inside a PR that Kade merged; those can be reversed.

## Status of an entry

- **Active**: in force.
- **Done**: decided and carried out.
- **Pending**: decided, not carried out yet (the roadmap stage is named).
- **Open**: not decided; waiting for someone.

## 1. Decisions of 2026-10-06 (the architecture review)

| ID | Decision | By | Why | Consequence | Status |
|---|---|---|---|---|---|
| D-1 | Local and Beginner tab-limit default is **1000** | Kade | the core and ui code already treat an unset limit as 1000; `default_tab_limits` said 500 for these two (R-4) | `default_tab_limits` 500 -> 1000 for Local and Beginner; defined once in `types` (stage S2c). Saved configs are unaffected | Pending (S2c) |
| D-2 | `MEMORY.md` is limited to **40 lines and 6 KB** | Kade (the size is Claude's recommendation, accepted: "I'll follow your recommendation") | the file had grown to 287 lines / 103 KB against a rule of about 40 lines (M-1) | shrink it, move details to `.memory/`, add a gate check that fails above the limit (S0a) | Done 2026-10-06: `MEMORY.md` is 32 lines / 3.6 KB, the old text is in `.memory/sessions/2026-10-06-memory-index-archive.md`, CI runs `.github/scripts/memory-check.sh` |
| D-3 | Smoke steps keep running after one fails, **but a release is blocked unless every step succeeds** | Kade | one red row hid five pipelines in the last run (W-13) | `if: always()` on the pipeline steps; `release.yml` keeps `needs: [check, build, smoke]` and the smoke job fails when any step fails (S0b, own PR) | Pending (S0b) |
| D-7 | Documents are written **in English** | Kade | Kade asked for English after the Korean first draft (#204) | `docs/` and `.memory/roadmap/` in English; user-facing release notes stay Korean (CLAUDE.md) | Done |

## 2. Test and release process (2026-10-05 / 06)

| ID | Decision | By | Why | Consequence | Status |
|---|---|---|---|---|---|
| D-6 | The smoke test runs **only from a pushed release tag** (or by manual `workflow_dispatch`) | Kade, 2026-10-06: "PR to rc or main MUST NOT trigger smoke. Only release tagged push" | the smoke job takes about 20 minutes and was the bottleneck of every bridge PR | a bridge or runbook change is proven on the real app only by a manual dispatch or the next tag; do not wait for a smoke run before moving on | Active |
| D-5 | **No UI click automation** for now | Kade, 2026-10-06 | `ui-preview` is the UI check; click automation is heavy and flaky | `tauri-driver` / WebDriver stays out of the roadmap | Active |
| D-8 | Firewall rule per exe (K6) **stays manual** | Kade, 2026-10-06 | the rule is made only with `--add-firewall-rule`; a person checks it | `capture-spike` adds a rule only when asked | Active |
| D-9 | The real llama-server is replaced in tests by a **stand-in server** | Kade, 2026-10-06: "just create mock server, which respond exact same as llama-server. Real server loads heavy GPU" | a hosted runner has no GPU | `--llama-url`, `runbook/runbook/llama_stub.py`, pipeline `translator-stub` | Done |
| D-10 | The bridge is a **named command list**; Python defines the pipelines | Kade, 2026-10-05 (`MEMORY.md`: "Python orchestrates and defines pipelines, named command allowlist") | no generic remote control of the app | `Command` in `crates/core/bridge.rs`; every command has parse tests | Active |
| D-11 | The bridge and its MQTT client are compiled **only with the `test-env` feature** | Kade, 2026-10-06 ("Yes, and rest of test enhancements from latest handoff, without UI test automation") | a stable exe must contain none of it | `cargo tree` shows no `rumqttc` without the feature; a release workflow test pins that the stable build never mentions `test-env` | Done |
| D-12 | Stable releases are built and signed by `release.yml` only; **never publish by hand** | CLAUDE.md | a hand-made release becomes "latest" and updates silently stop | `release-feed-check.yml` watches the live feed | Active |
| D-13 | A release candidate (`rc`) is merged by a person; `rc` is never merged into `main` | CLAUDE.md | candidates need a run on Kade's Windows PC first | auto-merge never touches `rc` | Active |
| D-14 | One task = one branch = one PR, merged by CI when green; several tasks go one at a time | CLAUDE.md | keeps history and CI results attributable | `claude/*` PRs into `main` are auto-merged | Active |
| D-15 | The runbook is **docs**: no CI job checks it, no gate, no `NOT VERIFIED` line for it | CLAUDE.md | it changes no app logic and is run by hand | a courtesy `python -m pytest -q` in `runbook/` | Active |

## 3. Architecture and code

| ID | Decision | By | Why | Consequence | Status |
|---|---|---|---|---|---|
| D-4 | Keep **two `AppConfig` types** (the app's and the ui's) | Kade, 2026-09-29 (CLAUDE.md: "do not merge them") | the app owns the file and the real defaults; the ui sees its own view | a field added to one is added to the other with the same name; `app_config_full.json` round-trips in both | Active |
| D-16 | New pure logic goes in `crates/core`; what crosses the Tauri boundary is defined once in `crates/types` | CLAUDE.md | tested on every OS; one definition | the layering of the review's section 2 | Active |
| D-17 | No Npcap / WinDivert; capture is a raw socket (`SIO_RCVALL`) | CLAUDE.md (removed 2026-09-29) | capture never used them | needs Administrator; port 5003 | Active |
| D-18 | Favorites change only through `save_favorites`; `save_config` ignores the favorites in its payload | CLAUDE.md | a popup window's copy of the settings may be older | `CP-fav-*` rows check it on the real exe | Active |
| D-19 | reqwest **0.12**, not 0.13 | Claude, PR #199 (merged) | 0.13's TLS feature uses `aws-lc-rs`, a C build on Windows that could not be tested from Linux; 0.12 keeps `ring` | four advisories closed; tauri still brings 0.13 into the tree | Done (reversible) |
| D-20 | The MQTT packet limit of the test bridge is **16 MiB**, and an over-limit answer becomes an error ack | Claude, PR #203 (merged) | the 10 KiB default blocked a 108 KB answer and made the client reconnect | `MAX_PACKET_BYTES`, `fit_ack` | Done |
| D-21 | Golden tests **leave furigana out** until K16 is settled | Claude, PR #201 (merged) | a snapshot would freeze a reading nobody has chosen | `golden_text.rs` has no furigana table | Active |
| D-22 | Mutation testing and `cargo audit` are **not in `ci.yml`** | Claude, in the PR that added each workflow (`audit.yml`, then PR #202; merged) | a new advisory or a list of missed mutants must never hold up auto-merge | weekly workflows; a red run is a to-do | Active |
| D-23 | The app's own update comes from the newest stable release's `latest.json`, checked against built-in signing keys; the gist's `app` entry is ignored | CLAUDE.md | a feed host alone cannot get code run | `TRUSTED_UPDATE_KEYS` (a primary and an offline backup) | Active |
| D-24 | `Lindera.dev` is a trusted host (the furigana dictionary is built from it) | CLAUDE.md (trusted since 2026-10-03) | the build script downloads from it | a blocked host is asked for, never worked around | Active |

## 4. Open decisions

| ID | Question | Waiting for | Notes |
|---|---|---|---|
| O-1 | **K8**: the answer about the `<bos>` count in the prompt | Kade; the answer waits for the fine-tuning notebook (resonance-lab) | the stand-in only reports the literal `<bos>` count of every prompt |
| O-2 | **K16**: how the built exe reads 一人 / 二人 / 一人前 | the real-app result of `CR-ruby-probe` (stage S0c), then Kade | until settled, furigana stays out of the golden tests (D-21) |
| O-3 | Does the update check work on the real app? The update dialog did not show on 2026-10-06 | Kade: compare the app's version with the newest stable tag and read the system log's update line | either "already up to date" or a broken check; not told apart |
| O-4 | What should a leaked `<start_of_turn>model` become in the output? (W-12) | Kade | today it leaves the word "model" |
| O-5 | Should the app archive on arrival (A-3.1)? | Kade's go for stage S2a | changes where chat is written |
