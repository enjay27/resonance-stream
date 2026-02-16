# 🛠️ Resonance Stream Build Guide

This document provides the technical instructions required to compile and package **Resonance Stream** from source code. Since this project combines a Rust (Tauri) backend with a Python (AI Engine) sidecar, the build order and dependency management are critical.

## 📋 1. Prerequisites

Ensure your development environment meets these requirements before starting the build:

* **Rust**: Version 1.70+ with the **2021 Edition** toolchain.
* **Node.js & Trunk**: Required for building and bundling the **Leptos** frontend.
* **Python 3.10+**: Must have `ctranslate2`, `pykakasi`, `argparse`, and `pyinstaller` installed (via `requirements.txt`).
* **CUDA Toolkit**: Version 11.x or 12.x is required for GPU-accelerated translation.

---

## ⚙️ 2. Library & Dependency Setup (Automated)

Unlike previous versions, you do **not** need to manually download SDKs or configure a `.env` file. The project includes a bootstrap script to handle this automatically.

1. **Run the Setup Script**:
   Execute `setup_libs.bat` in the project root.
   ```cmd
   setup_libs.bat
   ```

- This script creates a `lib/` directory and automatically downloads/extracts the required **Npcap SDK** and **WinDivert** binaries.
- The `lib/` directory is git-ignored to keep the repository clean.

------

## 🚀 3. Compilation Pipeline (Manual Steps)

If you are building manually without `package.bat`, you must follow this exact sequence to satisfy Tauri's bundling requirements.

### Step 1: Library Path Setup

Temporarily set the environment variable so the Rust linker can find the Npcap library.

```
set LIB=%CD%\lib\npcap-sdk\Lib\x64;%LIB%
```

### Step 2: AI Sidecar Build (Python)

Use PyInstaller to compile the AI engine. Tauri requires the sidecar filename to include the **Target Triple**.

- **Command**:

```
pyinstaller --noconfirm --clean --distpath src-tauri\bin translator.spec
```

- **Output**: Generates `src-tauri/bin/translator-x86_64-pc-windows-msvc.exe`.

### Step 3: Driver Placement

Copy the WinDivert drivers from the `lib` folder to the binary location.

- `lib\WinDivert\x64\WinDivert.dll` -> `src-tauri\bin\`
- `lib\WinDivert\x64\WinDivert64.sys` -> `src-tauri\bin\`

### Step 4: Main Application (Rust/Tauri)

Bundle the frontend assets and compile the Rust backend to create the installer.

```
cargo tauri build
```

------

## 📦 4. Automated Packaging (Recommended)

The **`package.bat`** script automates the entire process and ensures a clean release. It performs the following:

1. **Dependency Check**: Verifies `lib/` exists; runs `setup_libs.bat` if missing.
2. **Sidecar Build**: Compiles the AI engine using `translator.spec`.
3. **Resource Placement**: Automatically copies WinDivert drivers to the correct build folder.
4. **Installer Build**: Runs the Tauri (NSIS) build process.
5. **Move & Organize**: Moves the final installer (`*-setup.exe`) to a clean **`dist`** folder in the project root for immediate deployment.

**How to Run:**

```
package.bat
```

------

## ⚠️ 5. Vital Runtime Notes

- **Administrator Privileges**: The final executable **must be run as Administrator** to load the WinDivert network driver.
- **Model Downloads**: On the first launch, the application will use the links in `models.json` to automatically download the required AI model files.
- **VC++ Redistributable**: Users must have the **Microsoft Visual C++ Redistributable (x64)** installed for the Python AI engine to initialize correctly.
- **Debugging Sidecar**: If translation fails, enable `console=True` in `translator.spec` and rebuild to see Python error logs.