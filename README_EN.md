# 🛰️ Resonance Stream (English)

**Blue Protocol: Star Resonance (BPSR) Real-time Packet-Sniffing Translator**

**Related: [Resonance Lab](https://github.com/enjay27/resonance-lab) | [LLM Model](https://huggingface.co/enjay27/TranslateGemma-Blue-Protocol-Translator-JA-KO)**

> This is a translation of [README.md](README.md) (Korean), which is the reference version.

---

## 📖 Introduction
![normal-mode](https://github.com/user-attachments/assets/a78ff1c9-ddc7-4c62-8104-162e397f8512)


![compact-mode](https://github.com/user-attachments/assets/733550a1-f05e-49b4-9b5a-2ab83756fdb7)

**Resonance Stream** sniffs the network packets sent by the Blue Protocol game server to extract and translate chat. This brings the game chat into its own UI, with features such as nickname search, message search and keyword alerts. Japanese chat that needs translating is translated in real time by a built-in AI model, fine-tuned on in-game chat data so that it uses the terms players actually use. [Project link](https://github.com/enjay27/resonance-lab)

### ✨ Key Features

* **Chat server packet sniffing**: Reads the chat sent by the Blue Protocol game server using a raw socket.
* **AI translation engine**: Uses a TranslateGemma-4B model fine-tuned on real Japanese chat data, so translations use in-game terminology.
* **Nickname reading**: Adds the reading of Japanese nicknames in parentheses next to the nickname.
* **Wide hardware support**: CPU / GPU mode and GPU offload tiers (Low, Middle, High, Very High) to match your hardware. Uses Vulkan, so GPUs from any vendor work.
* **Extra UI features**: Features the in-game chat does not offer (chat search, nickname filter, keyword alerts).

---

## 🛠️ Setup & Requirements

### Prerequisites

1. **Administrator rights**: The program must be run as Administrator to capture packets.
2. **Network firewall rule**: Packet capture needs a firewall rule. On first launch the app warns about it; if you decline, app features are limited. The rule is removed when the program exits.

### Installation

1. Download `resonance-stream.exe` from the latest [Release].
2. Run `resonance-stream.exe` **as Administrator**.

---

## 🗑️ Uninstall Guide

With translation enabled, **Resonance Stream** uses about **2.3 GB** of data for translation. When uninstalling, please check the following.

1. **Open the app data folder**: Press the "Open App Data folder" button at the bottom of the settings window to open the folder with the additional files.
2. **Delete the data**: Remove all files in that folder.
3. **Manual check (optional)**: Delete the `%APPDATA%\com.enjay.bpsr.resonance-stream` folder directly.

---

## 🚀 Usage

1. On first launch, a setup window lets you choose whether to use translation and CPU/GPU mode. These can be changed later in the settings.
2. Start the game (Blue Protocol) and connect to a server; packet capture starts automatically.
3. If chat does not come through while the game is running, check that the title bar at the top shows a green ```SNIFFER ON```. If it is red, click it to run the recovery steps.
4. With translation enabled, check that it shows ```번역 ON``` (Translation ON).

---

## 🚨 Troubleshooting

* v0.4.0 added a feature that makes firewall and network adapter problems easy to fix.

#### Packet sniffing does not work

**[Packet sniffing troubleshooting](https://github.com/enjay27/resonance-stream/issues/12)**

#### Translation does not work

**[Translation troubleshooting](https://github.com/enjay27/resonance-stream/issues/13)**

---

## 📜 Changelog

### v0.4.0 (2026-03-09)

- **UI improvements**: User dictionary editing and auto-sync, tab-switch shortcut, unread message counts, more readable message boxes
- **Retry on errors**: Automatically finds the network adapter when packet sniffing fails
- **Translation model change**: Switched to a TranslateGemma model for better translation quality
- **Firewall warning**: Added a firewall warning on first launch
- **Update detection**: Detects the latest version of the app so users can update easily

### v0.3.1 (2026-03-03) - Official release

- **Translation engine change**: Removed the Python translation libraries and the Python dependency; translation now uses llama-server with Vulkan support, so any GPU can accelerate translation.
- **Translation model change**: Replaced CTranslate2's NLLB-200 with a fine-tuned Qwen3-4B model, so in-game Japanese chat translates into natural Korean.
- **Packet capture dependency removed**: Switched from WinDivert to a raw socket, so the program runs as a single executable with no dependency files or extra installs.
- **Nickname management**: Adds romaji readings for Japanese nicknames and caches nicknames so they are not translated when they appear in chat.
- **Debug system**: System log (Debug Mode) for checking the network driver and translation.

---

## 👤 Author

* **Enjay** ([kdkyoung@gmail.com](mailto:kdkyoung@gmail.com))

---
