# Docs that contradict the code

Found 2026-09-29 while planning the refactor.

- **Translation engine.** `README*.md` and `BUILD*.md` describe a Python/CTranslate2
  sidecar built with PyInstaller (`translator.spec`, `requirements.txt`). The code
  downloads a llama.cpp Vulkan server zip (`src-tauri/src/services/downloader/server.rs`,
  `AI_SERVER_ZIP_URL`) and talks to it on `127.0.0.1:8080` (`translator/core.rs`).
  Neither `translator.spec` nor `requirements.txt` is in the repo.
- **Build scripts.** `BUILD*.md` references `setup_libs.bat`, which is not in the repo,
  and says `package.bat` builds the sidecar and copies drivers; `package.bat` only runs
  `cargo tauri build` and moves the installer.
- **Settings footer says "Resonance Stream v2.0"** (`src/components/settings/mod.rs`)
  while the app is 0.4.0 and the title bar (now `CARGO_PKG_VERSION`) says v0.4.0.
  Left as is: changing visible text is Kade's call.
- **index.html `<title>` is "Tauri + Leptos App"** (template leftover). Left as is: not
  verified whether Tauri propagates the document title to the window/taskbar.
