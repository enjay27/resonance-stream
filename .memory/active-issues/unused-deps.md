# Dependencies no code uses

Found 2026-09-29 (`graft grep` / `grep` over `src-tauri/src`): no `use pcap`,
no `windivert::`. Capture goes through a raw socket with `SIO_RCVALL`
(`src-tauri/src/services/sniffer/network.rs`); `Npcap` appears only in the adapter
ignore list.

- `pcap = "2.4"` — unused. README/BUILD still tell users to install the Npcap SDK.
- `windivert = { version = "0.6", features = ["vendored"] }` — unused; the vendored
  feature compiles C on every Windows build. README calls WinDivert the capture method.

Not removed yet: removing them changes the build and the docs' claims, and needs a
Windows run to confirm capture still works. Candidate for phase 5, with Kade's OK.
