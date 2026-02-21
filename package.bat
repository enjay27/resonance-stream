@echo off
setlocal enabledelayedexpansion

:: --- Configuration ---
set APP_NAME=Resonance-Stream
set VERSION=0.1.0
set OUTPUT_DIR=dist
set LIB_DIR=lib
set TARGET_BIN_DIR=src-tauri\bin
set BACKEND_SOURCE_DIR=src-tauri

:: [1/5] Check & Install Dependencies (Automated)
if not exist "%LIB_DIR%\npcap-sdk" (
    echo [INFO] Libraries not found. Running setup script...
    call setup_libs.bat
)

:: Set the LIB path for Rust Linker to our local lib folder
:: This removes the need for .env / NPCAP_PATH
set LIB=%CD%\%LIB_DIR%\npcap-sdk\Lib\x64;%LIB%

:: [2/5] Python Environment Setup
echo [2/5] Setting up Python environment...
if not exist ".venv.build" (
    python -m venv .venv.build
)
call .venv.build\Scripts\activate
pip install -r src-python\requirements.txt

:: [3/5] Build Dependencies
echo [3/5] Building Sidecar & Copying Drivers...

:: A. Build Python Sidecar
:: Note: Changed output to "%TARGET_BIN_DIR%" to match where WinDivert goes
PyInstaller --noconfirm --distpath "%TARGET_BIN_DIR%" --clean translator.spec

:: B. Copy WinDivert Drivers (From local lib to target bin)
echo Copying WinDivert drivers...
copy /Y "%LIB_DIR%\WinDivert\x64\WinDivert.dll" "%TARGET_BIN_DIR%\" >nul
copy /Y "%LIB_DIR%\WinDivert\x64\WinDivert64.sys" "%TARGET_BIN_DIR%\" >nul

:: [4/5] Build Tauri Installer
echo [4/5] Building Windows Installer...
call cargo tauri build

:: [5/5] Move & Organize
echo [5/5] Finalizing Release...

:: Create the distribution folder if it doesn't exist
if not exist "%OUTPUT_DIR%" mkdir "%OUTPUT_DIR%"

:: Robust Move: Searches for the setup file in case the target folder structure varies
set FOUND_INSTALLER=0
for /r "target\release\bundle\nsis" %%f in (*-setup.exe) do (
    echo Found: %%f
    move /Y "%%f" "%OUTPUT_DIR%\"
    set FOUND_INSTALLER=1
)

echo.
echo ========================================================
echo  Build Complete!
echo  Installer is ready in: %CD%\%OUTPUT_DIR%
echo ========================================================
pause