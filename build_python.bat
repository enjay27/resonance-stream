@echo off
setlocal enabledelayedexpansion

:: --- Configuration ---
set TARGET_BIN_DIR=src-tauri\bin

:: [1/5] Python Environment Setup
echo [1/5] Setting up Python environment...
if not exist ".venv.build" (
    python -m venv .venv.build
)
call .venv.build\Scripts\activate
pip install -r src-python\requirements.txt

:: [2/5] Build Dependencies
echo [3/5] Building Sidecar & Copying Drivers...

:: Ensure the target binary folder exists
if not exist "%TARGET_BIN_DIR%" mkdir "%TARGET_BIN_DIR%"

:: A. Build Python Sidecar
:: Note: Changed output to "%TARGET_BIN_DIR%" to match where WinDivert goes
PyInstaller --noconfirm --distpath "%TARGET_BIN_DIR%" --clean translator.spec

echo.
echo ========================================================
echo  Python Build Complete!
echo ========================================================
pause