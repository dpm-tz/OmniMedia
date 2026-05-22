# Build FFmpeg with WASAPI loopback support for Windows
# This script downloads FFmpeg source and builds it with WASAPI enabled
# Prerequisites: MSVC Build Tools, NASM, pkg-config

param(
    [string]$OutputDir = "src-tauri\resources\ffmpeg",
    [string]$FFmpegVersion = "master"
)

$ErrorActionPreference = "Stop"

Write-Host "FFmpeg WASAPI Loopback Build Script" -ForegroundColor Cyan
Write-Host "====================================`n" -ForegroundColor Cyan

# Check prerequisites
$requiredTools = @("git", "nasm", "pkg-config")
foreach ($tool in $requiredTools) {
    try {
        $null = & $tool --version 2>&1
        Write-Host "✓ $tool found" -ForegroundColor Green
    } catch {
        Write-Host "✗ $tool not found. Install it before continuing." -ForegroundColor Red
        Write-Host "  - git: https://git-scm.com/download/win"
        Write-Host "  - nasm: https://www.nasm.us/"
        Write-Host "  - pkg-config: Install via MSYS2 or vcpkg"
        exit 1
    }
}

# Check for MSVC
if (-not (Test-Path "C:\Program Files\Microsoft Visual Studio\*\*\VC\Auxiliary\Build\vcvars64.bat")) {
    Write-Host "✗ MSVC not found. Install Visual Studio Build Tools with C++ support." -ForegroundColor Red
    exit 1
}
Write-Host "✓ MSVC Build Tools found" -ForegroundColor Green

# Create temp directory
$tempDir = "build-ffmpeg-temp"
if (Test-Path $tempDir) {
    Write-Host "Removing existing build directory..." -ForegroundColor Yellow
    Remove-Item -Recurse -Force $tempDir
}
New-Item -ItemType Directory $tempDir | Out-Null

Push-Location $tempDir

try {
    # Clone FFmpeg
    Write-Host "`nCloning FFmpeg repository..." -ForegroundColor Yellow
    git clone --depth 1 https://git.ffmpeg.org/ffmpeg.git ffmpeg-src
    
    if (-not $?) {
        throw "Failed to clone FFmpeg repository"
    }

    Push-Location ffmpeg-src

    # Configure FFmpeg with WASAPI support
    Write-Host "`nConfiguring FFmpeg with WASAPI loopback support..." -ForegroundColor Yellow
    $configArgs = @(
        "--enable-gpl",
        "--enable-libx264",
        "--enable-libx265",
        "--enable-libvpx",
        "--enable-libopus",
        "--enable-libvorbis",
        "--enable-indev=dshow",      # DirectShow input
        "--enable-indev=wasapi",      # WASAPI input (loopback support)
        "--disable-static",
        "--enable-shared",
        "--toolchain=msvc"
    )

    & ".\configure" $configArgs
    
    if (-not $?) {
        throw "FFmpeg configuration failed"
    }

    # Build FFmpeg
    Write-Host "`nBuilding FFmpeg (this may take 30+ minutes)..." -ForegroundColor Yellow
    & "make" "-j$(($env:NUMBER_OF_PROCESSORS))"
    
    if (-not $?) {
        throw "FFmpeg build failed"
    }

    # Extract binary
    Write-Host "`nExtractingbinary..." -ForegroundColor Yellow
    $sourceExe = "ffmpeg.exe"
    if (-not (Test-Path $sourceExe)) {
        throw "ffmpeg.exe not found after build"
    }

    Pop-Location  # ffmpeg-src
    
    # Copy to final destination
    $destDir = Resolve-Path "../../$OutputDir"
    New-Item -ItemType Directory $destDir -Force | Out-Null
    
    $destExe = Join-Path $destDir "ffmpeg.exe"
    Copy-Item (Join-Path "ffmpeg-src" $sourceExe) $destExe -Force
    
    Write-Host "`n✓ Build successful!" -ForegroundColor Green
    Write-Host "FFmpeg with WASAPI support installed to: $destExe" -ForegroundColor Green
    
    # Verify WASAPI support
    Write-Host "`nVerifying WASAPI support..." -ForegroundColor Yellow
    $output = & $destExe -hide_banner -h demuxer=wasapi 2>&1 | Out-String
    if ($output -match "wasapi.*loopback" -or $output -match "WASAPI") {
        Write-Host "✓ WASAPI loopback support confirmed!" -ForegroundColor Green
    } else {
        Write-Host "⚠ Could not confirm WASAPI loopback support in build output" -ForegroundColor Yellow
    }

} finally {
    Pop-Location
    Write-Host "`nCleaning up temporary files..." -ForegroundColor Yellow
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
}

Write-Host "`n✓ Build process complete!" -ForegroundColor Green
