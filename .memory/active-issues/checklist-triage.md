# Triage of `unverified-on-windows.md` (2026-10-06)

Every bullet of [`unverified-on-windows.md`](unverified-on-windows.md), sorted by **how it can be checked now**. The test bridge, the
stand-in servers and the hosted-runner smoke run (`.github/workflows/bridge-smoke.yml`, a real exe on `windows-latest`) have changed what a person
has to do by hand. Nothing was deleted from the source file: tick a bullet off there only when its row below says *covered* **and** Kade has seen a
green run (the smoke run's report, or his own `python -m runbook.run ...`).

Legend. **covered** = an automatic row already checks it on the real exe (runs: `capture-spike`, `chat-rules`, `persistence`, `download-integrity`,
`translator-stub`, `replay-chat`, `window-restore`, `interface`, `updater-mock`). **small** = needs only another row in an existing pipeline, no app change.
**command** = needs a new named bridge command (a few lines each, the recipe in `sessions/2026-10-06-automation-plan.md`). **preview** = the wasm UI in a browser
(`ui-preview` skill, Linux). **human** = needs a person: the game, a real click, a second monitor, a VPN, the clipboard, the anti-cheat.

## Covered, or covered up to a detail

| bullet | status | what checks it |
|---|---|---|
| CI workflow (phase 1) | covered | CI is green on every PR; the app builds, links and its tests run on Windows |
| Phase 2: app tests, linking, the app itself | covered | CI's app gate; the smoke run starts the real exe 20+ times |
| Phase 5a: version source | **small** | `app-started` / the status file carry the version: add a row "equals the workspace version" (NSIS installer name stays human) |
| Review fixes: model/app/AI-engine download (`.part` then rename) | covered (model, app); AI-engine zip hash: **command** (`download-server`) | `download-integrity` DI-*, `updater-mock` M5-M7 |
| Review fixes: archive in the app data folder, block/unblock | covered | `persistence` CP-archive / CP-config, `chat-rules` CR-block* |
| Review fixes: chat capture, long / bursty world chat | covered for the path; **small** for bursts | `capture-spike`; add a burst of 500 frames in one segment and in 1-byte segments |
| Review fixes: translation on/off, one llama-server, new port if 8080 is busy, Exit kills it | partly | `translator-stub` (on, restart); the real server's port / kill needs the real llama-server -> **human** (GPU) |
| Review fixes: dictionary edit applies without restart | **small** | `translator-stub` + `save_local_dictionary` as a command: the next prompt carries the term |
| Review fixes: restart sniffer | **command** (`restart-sniffer`) | then `capture-spike` shape: chat after the restart |
| feat/ui-improve: model download skips when the hash matches | covered | DI-skip |
| feat/ui-improve: restart with archiving reloads chat, no duplicates after re-login | covered (reload) / **small** (no duplicates: replay the same lines after the restart) | `persistence` + one more replay |
| feat/ui-improve: window position restored | covered | `window-restore` |
| feat/ui-improve: no silent dictionary sync unless auto-sync is on | **command** (`sync-dictionary` against the mock gist) | `mockfeed` already serves `/metadata.json` |
| Translation ledger: translation off, collect Japanese chat, turn it on -> CATCHING UP, newest 100 | **small** | `translator-stub`: replay lines *before* `start-translator`, expect `Catching Up` then the translations; the 100 limit via `translation_catch_up_limit` |
| Translation ledger: new chat during the catch-up; tier change mid-catch-up stops the old worker | **small** / **command** (`restart-translator`) | stub with a slow reply mode |
| Translation ledger: a restart reloads each caught-up message once | covered | `persistence` (reload) with translations in the log: **small** |
| Chat log retention (`chat_log_retention_days`) | **small** | `persistence`: write dated old `chat_logs/*.jsonl` + config, start, the old ones are gone, `dataset_raw.jsonl` untouched |
| N11/N12/N13: a busy WORLD day does not push GUILD/PARTY out of the reload | **small** | `persistence` with 300 WORLD lines + 5 GUILD lines and tab limits |
| llama client timeouts, an empty reply is a failed job, not run against a real llama-server | covered / **small** | `translator-stub` (500s), a stub `hang` mode for the 30 s timeout |
| Beginner channel: lines reach the right channel | covered (replay supports BEGINNER) | `chat-rules` shape; the tab itself -> **preview** |
| Core-review leftovers W8: translator retry when llama-server exits while loading | **small** / human | stub `health_mode=down` -> "Retrying" -> Error after 3 tries (90 s load timeout: slow); a real exiting server -> human |
| Core-review leftovers W9: sniffer backs off on a failing socket read | human (nothing makes a raw socket fail on demand) |
| Settings sidebar + window grow: `grow_window` / `restore_window` | covered at the hosted runner's 1024x720 screen; DPI scaling, a second monitor, the taskbar's work area -> **human** | `window-restore` |
| System-log dedup: idle lobby 2+ minutes -> one "No game traffic for 15s", then "(repeated N more times)" | **small** | `capture-spike` shape: sniffer on, no traffic for ~80 s, read the `system-event`s (also K5's watchdog and the VPN hint) |
| System-log dedup: the system tab no longer fills with "Polling .../health..." | **small** | `translator-stub` with log level trace: count `system-event`s |
| Route-based interface pick (K4): two adapters, a full-tunnel VPN, offline | covered for the pick (`interface`); the VPN case (issue #142) -> **human** |
| Signed app updates (v0.6.0), A1-A5 | covered except A1 (a real newer release hop) | `updater-mock` 44 pass |
| Favorite tabs: restart keeps the tabs and shortcuts | **command** (`save-favorites`, `get-favorites`) | the same `FavoritesState` the popup uses |
| Favorites as a popup + sync, no overwrite of settings | **command** (`save-favorites` + `save-config`-like check of `config.json`) for the no-overwrite rule; the real second window -> **human** |
| Japanese study view: `annotate_furigana`, `translation_view` | **command** (`annotate-furigana`) for the spans; the look -> **preview** |

## Needs new named commands (popups, tray, shortcuts), then automatic

`open-popup {kind}`, `snapshot-popup {kind}` (rect), `hide-popup`, `popup-shown` event (exists), `tray-toggle-always-on-top`, `tray-toggle-click-through`,
`global-tab-switch` (the events exist; the commands would call the same handlers). With them the real window mechanics are checkable on the runner:

| bullet | rows it would give |
|---|---|
| Cheat sheet as a popup (1)(2)(5)(6) | one window per kind; a second open focuses, never a second window; minimizing / closing the main window takes the popup with it; it comes back at its saved size and place |
| Popup polish (1)(2)(3)(5) | two hidden popups exist ~2 s after start; first open at the saved place (no jump); X hides, Alt+F4 hides; closing the main window ends the process (no `resonance-stream.exe` left) |
| Phase 3 (tray menu: four items, global tab shortcut) | the handlers run; the real tray click and the real key press -> **human** |

## Preview only (the wasm UI in a browser, no Windows needed)

Phase 4 click-through; main UI redesign (transparency at low opacity needs the real window -> human for that one); rc UI fixes; cheat sheet look; chat row menus
+ add to dictionary; favorites table editing; Esc rules; Beginner tab look; badges (search, unread). Most of these already have `ui-preview` screenshots.

## Human only (what is left for Kade)

1. **The game itself:** favorite shortcuts pasting into the game chat (anti-cheat may ignore `SendInput`), clipboard restore delay, the double press.
2. **A real second monitor / DPI scaling / the taskbar's work area** for `grow_window`.
3. **A full-tunnel VPN** (issue #142) and the real route pick with two live adapters.
4. **The real llama-server on a GPU:** the 8080-busy port pick, Exit killing it, a bad model file / a taken port (W8), memory use of the pre-created popup webviews.
5. **The tray and global shortcuts as clicks and key presses**, IME composition in a popup, drag regions, transparency at low opacity.
6. **K6, the firewall rule per exe** (kept manual on purpose, Kade 2026-10-06), K3b (keys in two places), A1 (a hop to a real newer release).

## Order if more is wanted

1. The **small** rows (all bridge-only, no app change): retention, busy-WORLD reload, ledger "translation on later", watchdog + log dedup, version row, burst frames.
2. `restart-sniffer`, `save-favorites` / `get-favorites`, `sync-dictionary` / `save-local-dictionary`, `annotate-furigana`.
3. The popup / tray commands.
