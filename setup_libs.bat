@echo off
setlocal enabledelayedexpansion

:: Define versions, URLs, and Expected Hashes
set NPCAP_URL=https://npcap.com/dist/npcap-sdk-1.13.zip
set WINDIVERT_URL=https://github.com/basil00/Divert/releases/download/v2.2.2/WinDivert-2.2.2-A.zip

:: SHA-256 for the WinDivert 2.2.2-A ZIP file
set EXPECTED_HASH=6A7A61E3476E056D4A66C20F0606B4C25497B49F7D0D9D59013BAA6B8A28E5F0

:: Create lib directory
if not exist "lib" mkdir "lib"

echo [1/2] Downloading Npcap SDK...
if not exist "lib\npcap-sdk" (
    powershell -Command "Invoke-WebRequest -Uri '%NPCAP_URL%' -OutFile 'lib\npcap.zip'"
    powershell -Command "Expand-Archive -Path 'lib\npcap.zip' -DestinationPath 'lib\npcap-sdk' -Force"
    del "lib\npcap.zip"
    echo    - Npcap SDK installed.
) else (
    echo    - Npcap SDK already exists.
)

echo [2/2] Downloading WinDivert...
if not exist "lib\WinDivert" (
    powershell -Command "Invoke-WebRequest -Uri '%WINDIVERT_URL%' -OutFile 'lib\windivert.zip'"

    :: --- INTEGRITY CHECK START ---
    echo    - Verifying file integrity...
    for /f "tokens=*" %%a in ('powershell -Command "Get-FileHash lib\windivert.zip -Algorithm SHA256 | Select-Object -ExpandProperty Hash"') do set ACTUAL_HASH=%%a

    if /i "!ACTUAL_HASH!" EQU "!EXPECTED_HASH!" (
        echo    - [SUCCESS] Hash matches.
        powershell -Command "Expand-Archive -Path 'lib\windivert.zip' -DestinationPath 'lib' -Force"
        move "lib\WinDivert-2.2.2-A" "lib\WinDivert"
        del "lib\windivert.zip"
        echo    - WinDivert installed.
    ) else (
        echo    - [ERROR] Hash mismatch!
        echo      Expected: !EXPECTED_HASH!
        echo      Actual:   !ACTUAL_HASH!
        del "lib\windivert.zip"
        echo [FATAL] Download corrupted. Please run the script again.
        exit /b 1
    )
    :: --- INTEGRITY CHECK END ---
) else (
    echo    - WinDivert already exists.
)

echo.
echo Dependencies are ready in /lib
pause