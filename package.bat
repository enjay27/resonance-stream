@echo off
setlocal enabledelayedexpansion

:: --- Configuration ---
set APP_NAME=Resonance-Stream
set VERSION=0.2.0
set OUTPUT_DIR=dist

:: [1/1] Build Tauri Installer
echo [1/1] Building Windows Installer...
call cargo tauri build

:: Finalizing Release
echo Finalizing Release...
if not exist "%OUTPUT_DIR%" mkdir "%OUTPUT_DIR%"

set FOUND_INSTALLER=0
for /r "src-tauri\target\release\bundle\nsis" %%f in (*-setup.exe) do (
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