# Unverified on Windows

The Linux cloud sessions cannot build `src-tauri/` (WinDivert, `windows-sys`, GTK
missing). Anything listed here compiled only in CI, or not at all, and has not been
run as the real app.

- **CI workflow (phase 1).** `.github/workflows/ci.yml` has never run. The Windows
  job assumes no Npcap SDK is needed: `pcap` 2.5's build script falls back to version
  1.0.0 when `wpcap.dll` is absent, and nothing links it because no code uses the crate
  (see `unused-deps.md`). If the job fails on linking, that assumption is the first suspect.
  It also relies on a placeholder `dist/index.html` for `generate_context!`.
