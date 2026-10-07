# Handoff -- the architecture review and refactor roadmap, after S0-S6 and two steps of signed metadata (2026-10-07)

For a new session with no context. Read `CLAUDE.md` (plan first, test first, one task = one branch = one PR, wait for the merge before the next,
never report a gate as passed when it could not run, a blocked host is asked for), `MEMORY.md`, then
[`../roadmap/refactor.md`](../roadmap/refactor.md) (every stage, with what proved it), [`../../docs/decisions.md`](../../docs/decisions.md) (D-25 to D-29 are
this session's), [`../../docs/architecture_review.md`](../../docs/architecture_review.md) and [`../../docs/security_model.md`](../../docs/security_model.md).
Kade is the user (Korean, a Windows PC with the game). He answers in short lines; "follow your recommendation" means do what you recommended.

## Where things stand

All of S0-S6 of the roadmap is merged (`main`), one PR at a time, each with its `NOT VERIFIED` lines:

| stage | PRs | what |
|---|---|---|
| S0 | #207-#209 | smoke steps independent, the real-app rows read |
| S1 | #210-#212 | update rollback, `.part` extraction, panic log |
| S2 | #214, #215, #216, #218, #219 | defaults in `types`, archive on arrival, settings read/write reported, `config_version`, `netsh` off the main thread; **#217 one `AppConfig`** (D-25) |
| S3 | #213, #220, #221 | server-file SHA-256 pins, timeouts and size caps, shell permissions removed + `open_browser` only https |
| S4 | #222-#226 | second-client dedupe in the pipeline (D-27), `sniffer_change` / `watchdog_check` in core, an `Error` state on every socket-setup failure, `Services` owns the translator and the sniffer |
| S5 | #227-#231 | text mutation gaps, `[P<n>]` collision, a leaked role header removed whole, one block list, nickname lock scope |
| S6 | #232-#236 | list limit shrinks, batched furigana, one scroll effect, **#235 a compact-mode click panic fix**, `ChatRow` split |
| S3b-2 | #237 (M1), #238 (M2) | signed metadata: the core verifier and `metadata/` sources; the signing workflow `metadata.yml` |

Real-app proof: the `bridge-smoke.yml` dispatch (`mcp__github__actions_run_trigger`, workflow `bridge-smoke.yml`, ref `main`; about 22 minutes) was green,
all eight pipelines, on `main` at #208, #226, #230 and #236 (runs 37545082187, 37567018818, 37573382046, 37588905867). Only step results were read, not
row values. **Nothing in S6 has been seen in a real window** (`cargo tauri dev`, Windows, Administrator); the ui was checked in headless Chromium
with the mock bridge (`ui-preview` skill).

Skipped by agreement: P-5 (parser `unknown_fields`), R-9 (`types` split); R-8 is mostly moot. `FavoritesWindow`, `DictionaryModal` and `NavBar` are
still long; Kade can ask for them.

## The signed metadata (D-28) -- what is next

Decided by Kade: the model's and the dictionary's metadata is published from this repo, signed with the app-update key, onto a generated `metadata`
branch; **signing runs only when Kade pushes a tag `metadata-v<N>` on a commit of `main`** (a merge must never sign: `claude/*` PRs auto-merge);
rollback guard on; a bad or missing signature refuses the update and keeps the last good copy, with an Error line in the system log; the gist stays
untouched for installed 0.6.x copies. Sources are `metadata/metadata.json` and `metadata/custom_dict.json` (Kade's gist content). The dictionary has no
signature of its own: the signed metadata carries its SHA-256.

**UPDATE (later the same day): `metadata-v1` is published and verified (see MEMORY.md); the tag must sit on a commit that already has `.github/workflows/metadata.yml` (#238 or later), or nothing runs. M3 is split M3a/M3b/M3c, Kade approved the plan; the paragraph below is the original note.** **Kade's first publish was the next thing, and it was his:** `git tag metadata-v1 <commit of main> && git push origin metadata-v1`. Then he checks Actions >
Metadata is green and the `metadata` branch holds `metadata.json`, `metadata.json.sig`, `custom_dict.json`, `README.md`. **Open unknown:** `tauri signer sign`
on a `.json` file was never run (it is the release workflow's command, and signs any file). If the run is red, read the failed step first. **Do not start M3
before he says it worked.**

M3 (the app; plan first, tests first), design notes so you need not rediscover them:
- Where: `src-tauri/src/services/downloader/gist.rs` (`check_all_updates`, `sync_dictionary`, `METADATA_URL`, `DICT_URL`). Use
  `resonance_core::signed_metadata::{verify_published, verify_metadata_with_app_keys}`; it is pure and tested (17 tests, mutation-checked).
- URLs: `https://raw.githubusercontent.com/enjay27/resonance-stream/metadata/{metadata.json,metadata.json.sig,custom_dict.json}` (GitHub's cache can hold an
  old copy for minutes).
- Remember the highest accepted revision in the local metadata file (`config::load_metadata` / `save_metadata`: add a field with a serde default); pass it
  as `accepted_revision`; write it only after the whole publication verified.
- A failed check: the model and dictionary updates are refused, the installed dictionary and model stay, one Error line goes to the system log; the app update
  check (its own signed feed) is unaffected. The ui gets `UpdateCheckResult` as today.
- Test flags (`crates/core/src/test_env.rs` already has `--metadata-url`, `--dictionary-url`): add the signature URL and a **test-only trust-key flag** so the
  bridge can use a throwaway key. `release.yml` must never name a test flag (a test pins it), and the flag must not exist in a stable exe. Bridge rows go in the
  translator-stub pipeline's `TS-dict-*` neighbourhood; `runbook/tests/fake_app.py` is the stand-in app (docs, no CI).
- Core already has `GistMetadata.revision` and `RemoteDictionary.sha256` (serde default, skipped when empty: the old gist's shape and the wire-format test did
  not change).
- M4 (optional): `download_model` takes URL and hash from the UI, which got them from the metadata; use the last verified metadata instead.
- The new model goes out through the signed path: edit `metadata/metadata.json` (and tag) when hy-mt2 is chosen.
- One thing to check in the data: `ギルミーメイジ` -> `리자드맨 마법사사` in `custom_dict.json` looks like a typo for `마법사`.

## Other things waiting on Kade

- **S3c-2, the webview CSP, goes through `rc`** (a wrong CSP blanks the window and WebView2 cannot run on Linux): branch `candidate/webview-csp`, PR into `rc`, Kade merges it
  and tests the exe, then the same branch to `main`. The proposed policy is in the roadmap (S3c-2). Kade said he will test after he is home. Check it first in headless Chromium
  through the preview server.
- **S7, the sanitised capture corpus**: needs Kade's real capture; a substitution tool plus a replay in CI (`capture_replay.rs`).
- **K16** (how 一人 / 二人 / 一人前 are read): he will upload reference readings from another Claude session; the real-app result is the `CR-ruby-probe` row of the
  bridge-smoke logs (artifact `bridge-smoke-logs`, 7 days). Furigana stays out of the golden tests until it is settled (D-21).
- **O-3, the update check**: no dialog is expected while the app is 0.6.1 = the newest stable release; only a newer release shown to an older copy proves it.
- **D-29, the base model moves from TranslateGemma to hy-mt2** (K8, the `<bos>` count, is moot). Nothing is known here about hy-mt2's prompt. Gemma-specific code to revisit when it is
  chosen: `translation_prompt` and its pinned test, `sanitize_input`, `TURN_TAG_PATTERN` and `ROLE_HEADER_PATTERN` (`text.rs`), the stop tokens in `completion_request`, the prompt goldens
  (`crates/core/tests/golden_text.rs`), the model's hash and size in the metadata, and whether the pinned llama.cpp build (`server_pins`, `.github/scripts/ai-server-pins.py`) runs it.
- Every S6 change needs a look in `cargo tauri dev`: the list limit (scroll up, back down), furigana in the study view, translations landing at the bottom, the `ChatRow` menus,
  compact mode (click anywhere; it used to panic).

## How the work was done (so you can keep doing it)

- One task, one branch `claude/<name>` from `origin/main`, tests first (see them fail for the right reason), gates, memory update in the same branch, PR, `subscribe_pr_activity`,
  a `send_later` check-in about 50 minutes out; on "merged" cancel it (`delete_trigger`), check the branch is gone, then start the next from `origin/main`. Commit bodies say what is verified
  and what is `NOT VERIFIED`. Plan first for anything with a real design choice; Kade confirms.
- Gates here: `cargo fmt --check`; `cargo test -p resonance-core -p resonance-llama -p resonance-types`; `cargo test -p resonance-stream-ui` and
  `cargo check -p resonance-stream-ui --target wasm32-unknown-unknown`; the app only as a compile check: `cargo check -p resonance-stream --target x86_64-pc-windows-gnu` (plain,
  `--features test-env --tests`, `--release --features test-env`); `bash .github/scripts/memory-check.sh`; the shell tests `bash .github/scripts/*.test.sh`.
  The environment needed `rustup target add x86_64-pc-windows-gnu wasm32-unknown-unknown`, `gcc-mingw-w64-x86-64`, `cargo install cargo-mutants`, `pip install actionlint-py`, and for the
  preview `playwright` and `Pillow`.
- Mutation testing: `cargo mutants -p resonance-core --file <file> --no-shuffle -j 2 --output <dir>` (about 5 minutes for `text.rs`); a surviving mutant is a missing test, or an equivalent
  one to remove by restructuring. A tie decided by a `HashMap`'s order passes by luck: use enough items.
- Golden tests use `insta`; `INSTA_UPDATE=always cargo test -p resonance-core --test golden_text` rewrites snapshots, then read `git diff` and delete any `*.snap.new`.
- `ui-preview` (`.claude/skills/ui-preview`): `preview.sh <dir>` serves the ui at `127.0.0.1:8765` (about 20 s after the first build); drive it with Playwright. The mock bridge's `invoke`
  receives its arguments as a `Map`; `listen` never fires, so capture handlers by wrapping `window.__TAURI__` in an init script (a setter on `window.__TAURI__`). For a refactor, take the same states
  before and after and compare PNGs (two runs of one build differ by up to ~100 pixels).
- **leptos 0.8's `window_event_listener` is not removed when its component goes**: use `utils::window_listener` (removes it with the component).
- Hosts the session cannot reach: the gist hosts (403 from the proxy). Do not work around a blocked host; ask Kade.
- The Stop hook asks to commit and push when the tree is dirty: commit locally when a check is still running, push after it passes.

## First message for the new session

"Read `.memory/sessions/2026-10-07-architecture-roadmap-handoff.md`, `MEMORY.md` and `.memory/roadmap/refactor.md`. Tell me what you read, then wait for what I ask (I may report the first
`metadata-v1` publish, the CSP test or the capture)."
