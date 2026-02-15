# 🛰️ Resonance Stream (English)

**Real-time Packet-Sniffing Translator for Blue Protocol: Star Resonance (BPSR)**

> A sophisticated utility that captures Japanese game packets in real-time and translates them into high-quality Korean using AI.

---

## 📖 Introduction

**Resonance Stream** extracts and translates chat content by sniffing network packets without modifying the game client (No Hooking). This ensures account security while providing a seamless play experience on Japanese servers. It features a powerful AI inference engine optimized for high-performance GPUs like the **RTX 4080 Super**.

### ✨ Key Features

* **Non-Invasive**: Uses WinDivert for packet sniffing, ensuring no interference with the game client.
* **High-Performance AI**: Utilizes a Python sidecar based on CTranslate2 for low-latency, high-quality translation.
* **Optimized Build**: Minimal resource footprint with a 39MB ultra-lightweight engine.
* **Performance Tiers**: Provides 4 levels of performance options (Low ~ Extreme) tailored to user hardware.
* **Intelligent Post-processing**: Includes Korean particle (Josa) correction and custom nickname management.

---

## 🛠️ Setup & Requirements

### Prerequisites

1. **Administrator Privileges**: Required for the packet sniffer to function correctly.
2. **MSVC++ Redistributable (x64)**: Mandatory for running the Python translation engine. [Download](https://www.google.com/search?q=https://aka.ms/vs/17/release/vc_redist.x64.exe).
3. **NVIDIA GPU**: Latest drivers are recommended for the best experience via CUDA acceleration.

### Installation

1. Download the `.zip` file from the latest [Release] section.
2. Extract it and ensure all files (`translator.exe`, `WinDivert64.sys`, etc.) are in the same folder.
3. Run `translator.exe` as **Administrator**.

---

## 🚀 How to Use

1. Launch the app and select a **Performance Tier** in the **Settings (⚙️)** tab that matches your GPU.
2. Start the game (BPSR) and log in; packet detection will begin automatically.
3. Check the **System (⚙️)** tab for the `First Packet Captured!` message.
4. Translated text will appear on the main screen in real-time.

---

## 📜 Changelog

### v1.0.0 (2026-02-15) - Initial Release

- **Engine Optimization**: Fully removed PyTorch dependency to reduce sidecar size to 39MB.
- **Real-time Packet Sniffing**: Implemented high-speed packet sniffing via WinDivert.
- **AI Performance Tiers**: Added 4 performance levels (Low, Middle, High, Extreme).
- **Korean Post-processing**: Introduced automatic particle (Josa) correction for natural translation.
- **Nickname Manager**: Added functionality to convert and manage player nicknames into Romaji.
- **Diagnostics**: Integrated System Logs (Debug Mode) for real-time driver and network monitoring.

---

## 👤 Author

* **Enjay** ([kdkyoung@gmail.com](mailto:kdkyoung@gmail.com))

---