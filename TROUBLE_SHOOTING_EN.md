# 🛠️ Resonance Stream Troubleshooting Guide

This application utilizes a kernel-level driver (**WinDivert**) to capture game packets and runs an **AI Engine (Python Sidecar)** for real-time translation. Most execution failures are caused by permission issues, missing runtime components, or security software interference.

------

### ✅ Success Flow: Normal Operation Check

To verify if the app is running correctly, enable **Debug Mode** and ensure the following messages appear in order within the **System (⚙️)** tab:

1. **`[System] Admin Privileges: true`**: Successfully running with administrator rights.
2. **`[System] Driver Integrity: true`**: Required driver files detected.
3. **`[Python] AI Started: CUDA`**: GPU acceleration is active (e.g., **RTX 4080 Super**).
4. **`[Debug] New Stream Detected`**: Communication with the game server captured.
5. **`[Debug] First Packet Captured!`**: Analysis of actual game data has begun.

------

### 1. How to Check System Logs

Detailed diagnostic information and debug logs are available in the **System Tab**.

1. **Open Settings**: Click the **Gear Icon (⚙️)** at the top right of the app.
2. **Enable Debug Mode**: Scroll down and check the **'Debug Mode'** box.
3. **Enter System Tab**: Click the newly appeared **'System (⚙️)'** tab in the top navigation bar.

------

### 2. Troubleshooting by Log Type

#### 🖥️ [System] Logs (Initialization & Permissions)

- **`Admin Privileges: false`**: The program is running with standard user permissions. Packet sniffing will not function.
    - **Solution**: Fully close the app, then **Right-click `translator.exe` > [Run as administrator]**.
- **`Driver Integrity: false`**: `WinDivert64.sys` is missing from the execution folder.
    - **Solution**: Ensure all 5 required files are in the **same folder**. Re-extracting the `.zip` file is recommended if files are missing.
- **`Resource Path: ...`**: Displays the path where resources are loaded.
    - **Caution**: Errors may occur if the path contains non-English characters or special symbols. Please run the app from a **purely English directory path**.

------

#### 📡 [Sniffer] Logs (Network Detection)

- **`FATAL: ACCESS_DENIED (Code 5)`**: Failed to load the driver due to insufficient permissions.
    - **Solution**: You must restart the program as an **Administrator**.
- **`FATAL: INVALID_IMAGE_HASH (Code 577)`**: Windows Security or Secure Boot has blocked the driver.
    - **Solution**: Run Windows Update to the latest version or check your antivirus software's exclusion settings.
- **`[Warning] No game traffic detected for 15s`**: The game is running, but no packet data is being received.
    - **Solution**: Ensure that Windows Firewall or your antivirus is **allowing network access** for this application.

------

#### 🤖 [Sidecar / Python] Logs (AI Translation Engine)

- **`FATAL: Missing Visual C++ Redistributable`**: Essential runtime components are missing from your PC.
    - **Solution**: Click to download and install the **[Official Microsoft x64 Runtime](https://www.google.com/search?q=https://aka.ms/vs/17/release/vc_redist.x64.exe)**. You **MUST restart your PC** after installation.
- **`GPU Memory Out`**: The graphics card's VRAM is insufficient.
    - **Solution**: In Settings (⚙️), lower the **Tier** to `Middle` or `Low`. High-performance GPU users may still encounter this if other memory-intensive programs (like the game itself) are running simultaneously.
- **`AI Started: CUDA`**: AI acceleration using the GPU has started successfully.

------

#### 🔍 [Debug] Logs (Detailed Analysis)

- **`Active Interface: ...`**: List of network devices currently being monitored.
    - **Purpose**: Used to verify if the correct network adapter carrying game traffic is detected.
- **`New Stream Detected: [IP:Port]`**: Successfully captured the connection to the game server.
    - **Purpose**: Indicates the app has recognized the game process and is ready to read data.
- **`First Packet Captured!`**: Proves that actual game data is entering the app's internal buffer.
    - **Purpose**: Real-time translation results will start appearing on the screen from this point onwards.
- **`[Perf] Inference Time: ...ms`**: Time taken by the AI to translate a sentence.
    - **Purpose**: If this value remains consistently high, adjust the Performance Tier for a smoother experience.

------

### 💡 Tips for Requesting Technical Support

If the issue persists, please copy the entire content of the **System Tab** and provide it for review. Lines containing **`[Sniffer FATAL]`** or **`[Python CRASH]`** are the most critical clues for diagnosis.

------

**Once you have confirmed logs up to `First Packet Captured!`, you are all set to enjoy the translated gameplay!**

