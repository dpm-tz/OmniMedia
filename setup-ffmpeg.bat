@echo off
REM Simple FFmpeg downloader - Gets minimal build with audio support
REM Saves to: src-tauri\resources\ffmpeg\ffmpeg.exe

setlocal enabledelayedexpansion

set "DEST_DIR=src-tauri\resources\ffmpeg"
set "DEST_EXE=%DEST_DIR%\ffmpeg.exe"
set "TEMP_ZIP=%TEMP%\ffmpeg-min.zip"

echo.
echo FFmpeg Downloader (with WASAPI Loopback Support)
echo ================================================
echo.

REM Try to download from multiple mirrors
set "SUCCESS=0"

echo Downloading FFmpeg build from BtbN...
powershell -Command "try { Invoke-WebRequest -Uri 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip' -OutFile '%TEMP_ZIP%' -TimeoutSec 300 -ErrorAction Stop; Write-Host 'Download complete'; exit 0 } catch { Write-Host 'Download failed'; exit 1 }"

if !ERRORLEVEL! EQU 0 (
    echo.
    echo Extracting FFmpeg...
    if not exist "%DEST_DIR%" mkdir "%DEST_DIR%"
    tar -xf "%TEMP_ZIP%" -C "%DEST_DIR%"
    
    REM Find and move ffmpeg.exe
    for /r "%DEST_DIR%" %%F in (ffmpeg.exe) do (
        echo Found: %%F
        if not "%%~dpF"=="%CD%\%DEST_DIR%\" (
            move /Y "%%F" "%DEST_EXE%"
            echo Moved to: %DEST_EXE%
        )
    )
    
    del /Q "%TEMP_ZIP%"
    echo.
    echo Verifying WASAPI support...
    "%DEST_EXE%" -hide_banner -h demuxer=wasapi >nul 2>&1
    if !ERRORLEVEL! EQU 0 (
        echo ✓ FFmpeg with WASAPI loopback is ready!
        set "SUCCESS=1"
    ) else (
        echo Warning: WASAPI support not detected
    )
) else (
    echo Download failed. Try running again or install ffmpeg manually:
    echo   winget install FFmpeg
    exit /b 1
)

if !SUCCESS! EQU 1 (
    echo.
    echo Ready to record with system audio!
    exit /b 0
) else (
    exit /b 1
)
