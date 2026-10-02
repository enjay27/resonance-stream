# Roadmap — signed app updates, route-based interface pick (2026-10-02)

Two backend features from the 2026-09-30 review's "ideas, not defects" list. **Neither is
started.** Each is its own task (one branch, one PR; CLAUDE.md *Version Control*). They are
independent: do either first. Facts below were read from `main` at `1af65f8`.

## A. Route-based network interface pick -- DONE 2026-10-02 (`candidate/route-interface-pick`)

**Done:** shape 1-3 below as written (`pick_interface` in `crates/core/src/sniffer_net.rs`, glue `find_game_interface` in `network.rs`); question (a) decided in code -- a route through a virtual adapter is not trusted -- and pinned by a test; needs the Windows check in `unverified-on-windows.md`. **Still open:** (b), (c) below. The text that follows is the plan as written before the work.

**Today.** `find_game_interface_ip()` (`src-tauri/src/services/sniffer/network.rs:166`)
returns the *first* adapter, in the order `list_afinet_netifas()` gives, whose name does not
contain a keyword from a hard-coded blocklist (`Loopback`, `vEthernet`, `TAP`, `Tailscale`,
`WireGuard`, `OpenVPN`, `Radmin`, `Hamachi`, `ZeroTier`, `VMware`, `VirtualBox`, `WSL`,
`Npcap`) and whose IPv4 is not loopback / link-local. Used by `initialize_network_socket`
(same file, `:19`) when `config.network_interface` is empty; a manual IP overrides it.

**Why it is weak.** The game's traffic leaves by the adapter the OS *routes* it through, not
the first one in a list. With Ethernet and Wi-Fi both up, or a VPN not on the blocklist, the
raw socket binds to an adapter that never sees port 5003 and the sniffer reports "no
traffic" (the watchdog). The user's way out is the manual picker / troubleshooter dialog.

**Idea.** Ask the OS which local address it would use to reach a public host: `connect()` a
UDP socket to any routable address (nothing is sent) and read `local_addr()`; that is the
source IP of the default route. (`local_ip_address::local_ip()` does the same.) Keep the
blocklist only as the fallback when that fails (no default route / offline).

**Shape.**
1. Pure, in `crates/core` (tested on every OS): `pick_interface(candidates: &[(String, Ipv4Addr)],
   route_ip: Option<Ipv4Addr>) -> Option<Ipv4Addr>` -- the route's address when it is among
   the candidates and is not loopback / link-local, else the blocklist rule moved here
   unchanged. Test first: route wins over list order; route ip not in the list -> fallback;
   no route -> blocklist order; virtual adapter on the route (a VPN full tunnel) -> decide
   and pin it (open question below).
2. Glue in `network.rs`: ask the OS for the route address, call the core function, log which
   rule chose the adapter (the system tab already logs "Using ... Interface").
3. No UI or config change; `network_interface` (manual) keeps priority.

**Done when:** core tests green; app cross-check green; on Windows with two live adapters the
sniffer binds the routed one without the manual picker (NOT verifiable on Linux).

**Open questions.** (a) A full-tunnel VPN makes the *route* the VPN adapter, but the game's
packets are still seen on the physical one -- is the route right then? Needs a capture on a
VPN machine (`raw_capture` exists for this). (b) Pick by the game server's address instead,
once known? The server IP is only learned after traffic arrives, so it cannot choose the
first bind; at most it could re-pick after a watchdog trip. (c) The ui troubleshooter
(`src/components/network_troubleshooter.rs`) scans adapters one by one; it could try the
routed one first.

## B. DECIDED 2026-10-02 (Kade) -- signed plain-exe updates, in progress (`candidate/updater`)

**Requirement:** the app stays a **portable plain exe**. Rejected: `tauri-plugin-updater`'s
`install()` + NSIS (checked in tauri-bundler 2.10.1 `installer.nsi`: it installs to the registered
or default dir -- Program Files / LocalAppData -- with registry, uninstaller and shortcuts, so a
portable exe would silently become a second, installed copy). Rejected for now: the plugin's
`check`/`download` with our own swap (works -- `Update::download` verifies, `install` is separate --
but replaces code that works); `self_update` crate (not read).

**Chosen design (lifecycle-level, the format is the commitment):**
1. Signature = `tauri signer` / minisign over the exact exe; trusted comment carries `version:`.
   Verified with `minisign-verify` in `resonance_core::update_signature::verify_update` (pure,
   tested; **PR 1, done**): any key of a *list* may have signed (rotation), the signed version must
   equal the announced one (no rollback), a missing version is refused.
2. **Two keypairs** made by Kade (`cargo tauri signer generate`, self-generated -- there is no
   "official" key). Both public keys go in the app; primary private key = Actions secret
   (`TAURI_SIGNING_PRIVATE_KEY` + password); backup private key offline (NAS). Never committed.
   A TLS cert (DSM) is not a minisign key and rotates -- not usable.
3. Verify after download **and again right before the swap** (`restart_to_apply_update` renames
   whatever sits at `update_temp.exe` today).
4. **App-update info comes from GitHub Releases** (stable-release workflow publishes exe + `.sig`
   + manifest), not the gist; the gist keeps model + dictionary data.
5. Update dialog shows a failed download / bad signature (today `let _ = invoke(...)` hangs).
6. Transition: copies <= 0.6 have no verification; the first release that does reaches them over
   the old SHA-256 path, trusted once.
**PRs:** 1 verify (done) -> 2 keys embedded + app wiring (done, glue not built here) -> 3 stable release workflow, sign + publish
(written, never run) -> 4 the app reads `latest.json` instead of the gist + dialog errors (done). **Bridge:** v0.6.0 is the first signed release; the gist `app` entry stays pointing at it for copies that predate the signed updater (Kade edits it, after the release exists).

## B (original notes, kept for the facts). `tauri-plugin-updater`

**Today.** A hand-rolled update path, not a plugin:
- `check_all_updates` (`downloader/gist.rs:34`) reads the public gist; a newer
  `app.latest_version` (semver, never a downgrade; an ignored version is skipped) offers an update.
- `download_app_update` (`downloader/app_updater.rs:9`) downloads `app.download_url`
  (HTTPS only, length + SHA-256 checked in `resonance_core::download`) to `update_temp.exe`
  next to the running exe; it refuses when the gist publishes no SHA-256.
- `restart_to_apply_update` renames the running exe to `<name>.old`, the new one into place,
  spawns it, `app.exit(0)`. The app runs as Administrator, so the new exe does too.

**What the plugin would add.** *Authenticity*: today the gist is the only trust root -- whoever
can edit it can point `download_url` and the SHA-256 at any exe, and the app runs it as
Administrator. The plugin verifies a minisign signature against a public key in
`tauri.conf.json` (`plugins.updater.pubkey`), so a hijacked gist cannot push code.

**What it costs / why not just do it.**
- A signing key: `TAURI_SIGNING_PRIVATE_KEY` (+ password) must be a GitHub Actions secret --
  never committed (CLAUDE.md: zero hardcoded credentials); only the public key is committed.
  Kade creates it (`cargo tauri signer generate`); losing it strands every installed copy.
- The plugin updates from the **bundle** (`bundle.createUpdaterArtifacts`; targets today:
  `["nsis"]`) and a `latest.json` manifest with `url` + `signature`. Today's flow swaps a
  plain exe. Check which file `download_url` points at in the live gist before choosing.
- A release workflow does not exist for stable builds (`package.bat` -> NSIS in `dist/`,
  hand-published; `release-candidate.yml` builds plain-exe prereleases only). The plugin needs
  one that signs and uploads `latest.json`.
- Capabilities (`updater:default`), `tauri-plugin-updater` in `Cargo.toml`, registering it in
  `run()`; the UI's `AppUpdateModal` and the `download-progress` event ("앱 업데이트" string is
  matched in the UI) would be rewired to the plugin's progress events.
- Rollback: the `.old` backup is a feature of the current flow; the plugin has none.

**Cheaper step that gets most of the safety.** Sign the *gist metadata*, or publish the
SHA-256 from the GitHub release assets (the release page, not the gist) so the hash and the
file share a trust root other than the gist. Decide before committing to the plugin.

**Shape if chosen.** (1) Decision record with Kade: plugin vs. signed-hash step, installer vs.
plain exe. (2) Key + secret. (3) Stable-release workflow that signs and publishes
`latest.json`, tested on a throwaway tag (`rc-lib.sh`-style helpers are tested in CI; follow
that). (4) App wiring, behind the existing gist version check so old copies still update once.
(5) Retire `app_updater.rs`.

**Done when:** an installed old build updates itself to a signed release on Windows, and a
build whose file does not match the signature is refused. App glue: Windows-only, manual.

## Order and size

A: one session, one PR. B: a decision first, then about three PRs (key + workflow, wiring,
retire the old path). Neither blocks the current `rc` work.
