@echo off
setlocal enabledelayedexpansion

:: Define versions and URLs
set NPCAP_URL=https://npcap.com/dist/npcap-sdk-1.13.zip
set WINDIVERT_URL=https://github.com/basil00/Divert/releases/download/v2.2.2/WinDivert-2.2.2-A.zip

:: Create lib directory if it doesn't exist
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
    :: Download ZIP file
    powershell -Command "Invoke-WebRequest -Uri '%WINDIVERT_URL%' -OutFile 'lib\windivert.zip'"

    echo    - Extracting WinDivert...
    :: Extract and rename folder
    powershell -Command "Expand-Archive -Path 'lib\windivert.zip' -DestinationPath 'lib' -Force"
    move "lib\WinDivert-2.2.2-A" "lib\WinDivert"

    :: Cleanup
    del "lib\windivert.zip"
    echo    - WinDivert installed.
) else (
    echo    - WinDivert already exists.
)

echo.
echo Dependencies are ready in /lib