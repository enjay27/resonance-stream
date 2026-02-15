# 🛠️ Resonance Stream Build Guide

This document provides the technical instructions required to compile and package **Resonance Stream** from source code.

## 📋 1. Prerequisites

Ensure your development environment meets these requirements before starting the build:

* **Rust**: Version 1.70+ with the **2021 Edition** toolchain.
* **Node.js & Trunk**: Required for building and bundling the **Leptos** frontend.
* **Python 3.10+**: Must have `ctranslate2`, `pykakasi`, and `argparse` installed.
* **CUDA Toolkit**: Version 11.x or 12.x is required for GPU-accelerated translation.
* **SDK Placements**:
* **WinDivert SDK**: Place `WinDivert.dll`, `WinDivert.lib`, and `WinDivert64.sys` into `src-tauri/bin/`.
* **Npcap SDK**: Required for linking the packet capture features.

---

## ⚙️ 2. Environment Configuration (.env)

To support multiple developers with different SDK locations, local paths are managed via a `.env` file instead of hardcoded batch scripts.

1. **Create .env**: Copy `.env.template` to `.env` in the project root.
2. **Configure Path**: Open `.env` and set your specific Npcap SDK path:
```text
NPCAP_PATH=C:\Path\To\npcap-sdk-1.16
```

---

## 🚀 3. Compilation Pipeline (Manual Steps)

The Python sidecar must be compiled before the Tauri application to ensure the binary is available for bundling.

### Step 1: Optimized AI Sidecar (Python)

We use a `.spec` file to manage optimizations (excluding heavy modules like `torch`) and bundle `pykakasi` data.

* **Command**:
```powershell
pyinstaller --noconfirm --clean translator-x86_64-pc-windows-msvc.spec
```

* **Note**: The binary is built with the `x86_64-pc-windows-msvc` suffix as strictly required by Tauri v2 for architecture identification.

### Step 2: Main Application (Rust/Tauri)

* **Manifest**: Ensure `models.json` is in the project root. The Rust backend uses a `PathResolver` to load this manifest dynamically.
* **Command**:
```bash
cargo tauri build
```

---

## 📦 4. Automated Packaging

The **`package.bat`** script automates the entire process, including a critical "Clean Up" phase:

1. **Environment Loading**: Reads `NPCAP_PATH` from your local `.env`.
2. **Sidecar Build**: Compiles the Python engine into `src-tauri/bin/`.
3. **Tauri Build**: Bundles the application and resources (including `models.json`).
4. **User-Friendly Renaming**: Renames the technical `translator-x86_64-pc-windows-msvc.exe` to a simple **`translator.exe`** in the release folder for a cleaner end-user experience.

---

## ⚠️ 5. Vital Runtime Notes

* **Administrator Privileges**: The final executable **must be run as Administrator** to load the WinDivert network driver.
* **Model Downloads**: On the first launch, the application will use the links in `models.json` to download the required AI model files to `%APPDATA%`.
* **VC++ Redistributable**: Users must have the **Microsoft Visual C++ Redistributable (x64)** installed for the Python AI engine to initialize correctly.