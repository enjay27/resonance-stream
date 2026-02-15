@echo off
setlocal enabledelayedexpansion

:: --- Configuration ---
set APP_NAME=Resonance-Stream
set VERSION=1.0.0
set RELEASE_DIR=release_v%VERSION%
set ZIP_NAME=%APP_NAME%_v%VERSION%.zip

:: [0/5] Load Environment Variables from .env
if not exist ".env" (
    echo [ERROR] .env file not found!
    echo Please copy .env.template to .env and configure your paths.
    pause
    exit /b
)

for /f "usebackq tokens=1,2 delims==" %%A in (".env") do (
    set %%A=%%B
)

:: [Check] Verify NPCAP_PATH is now set
if "%NPCAP_PATH%"=="" (
    echo [ERROR] NPCAP_PATH is not defined in your .env file.
    pause
    exit /b
)

:: Apply to LIB for the linker
set LIB=%NPCAP_PATH%\Lib\x64;%LIB%

echo [1/5] Building Optimized Sidecar (39MB)...
:: We keep the specific name here for build tracking
pyinstaller --noconfirm --clean --onefile ^
    --distpath src-tauri\bin ^
    --name translator-x86_64-pc-windows-msvc ^
    --optimize 2 ^
    --exclude-module torch ^
    --exclude-module IPython ^
    --exclude-module notebook ^
    --exclude-module matplotlib ^
    --exclude-module tkinter ^
    --add-data "build_env\Lib\site-packages\pykakasi\data;pykakasi\data" ^
    src-python\main.py

echo [2/5] Building Main Application...
cargo tauri build

echo [3/5] Preparing staging area: %RELEASE_DIR%...
if exist %RELEASE_DIR% rd /s /q %RELEASE_DIR%
mkdir %RELEASE_DIR%

echo [4/5] Copying and Renaming Binaries...
:: Copy main app
copy "src-tauri\target\release\resonance-stream.exe" "%RELEASE_DIR%\"

:: Copy Sidecar AND RENAME to 'translator.exe'
copy "src-tauri\bin\translator-x86_64-pc-windows-msvc.exe" "%RELEASE_DIR%\translator.exe"

:: Copy Drivers
copy "src-tauri\bin\WinDivert.dll" "%RELEASE_DIR%\"
copy "src-tauri\bin\WinDivert64.sys" "%RELEASE_DIR%\"

echo [5/5] Finalizing documentation...
copy "README.md" "%RELEASE_DIR%\"
copy "README_EN.md" "%RELEASE_DIR%\"
copy "TROUBLE_SHOOTING.md" "%RELEASE_DIR%\"
copy "TROUBLE_SHOOTING_EN.md" "%RELEASE_DIR%\"
echo Please run resonance-stream.exe as Administrator. > "%RELEASE_DIR%\관리자권한으로_실행하세요.txt"

powershell -Command "Compress-Archive -Path '%RELEASE_DIR%\*' -DestinationPath '%ZIP_NAME%' -Force"

echo SUCCESS: %ZIP_NAME% created. Sidecar renamed to translator.exe.
pause