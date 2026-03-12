# replay-test.ps1 - Automated replay-based testing
# Usage: powershell -File replay-test.ps1 [replay-file]

param(
    [string]$ReplayFile = "testdata\replays\bots.WAgame"
)

$ErrorActionPreference = "Stop"

$waExe   = if ($env:OPENWA_WA_PATH) { $env:OPENWA_WA_PATH } else { "I:\games\SteamLibrary\steamapps\common\Worms Armageddon\WA.exe" }
$gameDir = Split-Path $waExe
$src     = "target\i686-pc-windows-msvc\release"
$logDir  = "testdata\logs"

# Resolve replay file to absolute path
if (-not [System.IO.Path]::IsPathRooted($ReplayFile)) {
    $ReplayFile = Join-Path (Get-Location) $ReplayFile
}

if (-not (Test-Path $ReplayFile)) {
    Write-Error "Replay file not found: $ReplayFile"
    exit 1
}

# 1. Build launcher + DLL
Write-Host "Building..." -ForegroundColor Cyan
cargo build --release -p openwa-launcher -p openwa-wormkit
if ($LASTEXITCODE -ne 0) { exit 1 }

$launcher = "$src\openwa-launcher.exe"

# 2. Clear old logs
Remove-Item "$gameDir\OpenWA.log"            -ErrorAction SilentlyContinue
Remove-Item "$gameDir\OpenWA_validation.log" -ErrorAction SilentlyContinue

# Remove old WormKit DLL if present from a previous deployment
if (Test-Path "$gameDir\wkOpenWA.dll") {
    Write-Host "Note: removing legacy $gameDir\wkOpenWA.dll" -ForegroundColor Yellow
    Remove-Item "$gameDir\wkOpenWA.dll"
}
if (Test-Path "$gameDir\wkOpenWAValidator.dll") {
    Remove-Item "$gameDir\wkOpenWAValidator.dll"
}

# 3. Launch via launcher with replay, env vars set
Write-Host "Launching WA.exe with replay: $ReplayFile" -ForegroundColor Cyan
Write-Host "  (Fast-forward mode: replay will be auto-advanced, game exits when done)" -ForegroundColor Yellow

$env:OPENWA_VALIDATE    = "1"
$env:OPENWA_REPLAY_TEST = "1"
$env:OPENWA_WA_PATH     = $waExe

# The launcher blocks until WA.exe exits, so run it as a background job for timeout support.
$proc = Start-Process -FilePath $launcher -ArgumentList "--minimized `"$waExe`" `"$ReplayFile`"" -PassThru
$timeout = 150  # seconds
if (-not $proc.WaitForExit($timeout * 1000)) {
    Write-Host "WARNING: launcher did not exit within ${timeout}s, killing..." -ForegroundColor Red
    $proc.Kill()
    $proc.WaitForExit(5000)
}
$env:OPENWA_VALIDATE    = $null
$env:OPENWA_REPLAY_TEST = $null
$env:OPENWA_WA_PATH     = $null

Write-Host "Launcher exited with code $($proc.ExitCode)" -ForegroundColor Cyan

# 4. Copy logs back
if (-not (Test-Path $logDir)) {
    New-Item -ItemType Directory -Path $logDir | Out-Null
}

$timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"

if (Test-Path "$gameDir\OpenWA_validation.log") {
    Copy-Item "$gameDir\OpenWA_validation.log" "$logDir\validation_$timestamp.log"
    Copy-Item "$gameDir\OpenWA_validation.log" "$logDir\validation_latest.log"
    Write-Host "Validation log copied." -ForegroundColor Green
} else {
    Write-Host "WARNING: No validation log found!" -ForegroundColor Red
}

if (Test-Path "$gameDir\OpenWA.log") {
    Copy-Item "$gameDir\OpenWA.log" "$logDir\openwa_$timestamp.log"
    Copy-Item "$gameDir\OpenWA.log" "$logDir\openwa_latest.log"
    Write-Host "OpenWA log copied." -ForegroundColor Green
}

# 5. Print summary
Write-Host ""
Write-Host "=== Validation Summary ===" -ForegroundColor Cyan
if (Test-Path "$logDir\validation_latest.log") {
    $content = Get-Content "$logDir\validation_latest.log" -Raw
    $passes = ([regex]::Matches($content, "\[PASS\]")).Count
    $fails  = ([regex]::Matches($content, "\[FAIL\]")).Count
    Write-Host "  PASS: $passes" -ForegroundColor Green
    Write-Host "  FAIL: $fails" -ForegroundColor $(if ($fails -gt 0) { "Red" } else { "Green" })

    if ($content -match "Safety timeout reached") {
        Write-Host "  Replay: TIMEOUT (safety timeout triggered)" -ForegroundColor Red
    } elseif ($content -match "deferred global validation") {
        Write-Host "  Validation: completed" -ForegroundColor Green
    } else {
        Write-Host "  Validation: NOT found in log (may not have reached gameplay)" -ForegroundColor Yellow
    }
}
Write-Host "  Logs: $logDir\" -ForegroundColor Gray
