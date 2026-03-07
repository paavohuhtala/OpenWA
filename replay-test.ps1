# replay-test.ps1 - Automated replay-based testing
# Usage: powershell -File replay-test.ps1 [replay-file]

param(
    [string]$ReplayFile = "testdata\replays\bots.WAgame"
)

$ErrorActionPreference = "Stop"

$gameDir = "I:\games\SteamLibrary\steamapps\common\Worms Armageddon"
$waExe = "$gameDir\WA.exe"
$src = "target\i686-pc-windows-msvc\release"
$logDir = "testdata\logs"

# Resolve replay file to absolute path
if (-not [System.IO.Path]::IsPathRooted($ReplayFile)) {
    $ReplayFile = Join-Path (Get-Location) $ReplayFile
}

if (-not (Test-Path $ReplayFile)) {
    Write-Error "Replay file not found: $ReplayFile"
    exit 1
}

# 1. Build
Write-Host "Building..." -ForegroundColor Cyan
cargo build --release -p openwa-wormkit -p openwa-validator
if ($LASTEXITCODE -ne 0) { exit 1 }

# 2. Deploy
Write-Host "Deploying to $gameDir..." -ForegroundColor Cyan
Copy-Item "$src\openwa_wormkit.dll" "$gameDir\wkOpenWA.dll"
Copy-Item "$src\openwa_validator.dll" "$gameDir\wkOpenWAValidator.dll"

# 3. Clear old logs
Remove-Item "$gameDir\OpenWA.log" -ErrorAction SilentlyContinue
Remove-Item "$gameDir\OpenWA_validation.log" -ErrorAction SilentlyContinue

# 4. Launch WA.exe with replay, env var set
Write-Host "Launching WA.exe with replay: $ReplayFile" -ForegroundColor Cyan
Write-Host "  (Auto-capture mode: will exit after 5s)" -ForegroundColor Yellow

$env:OPENWA_REPLAY_TEST = "1"
$proc = Start-Process -FilePath $waExe -ArgumentList "`"$ReplayFile`"" -WindowStyle Minimized -PassThru
$proc.WaitForExit()
$env:OPENWA_REPLAY_TEST = $null

Write-Host "WA.exe exited with code $($proc.ExitCode)" -ForegroundColor Cyan

# 5. Copy logs back
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

# 6. Print summary
Write-Host ""
Write-Host "=== Validation Summary ===" -ForegroundColor Cyan
if (Test-Path "$logDir\validation_latest.log") {
    $content = Get-Content "$logDir\validation_latest.log" -Raw
    $passes = ([regex]::Matches($content, "\[PASS\]")).Count
    $fails = ([regex]::Matches($content, "\[FAIL\]")).Count
    Write-Host "  PASS: $passes" -ForegroundColor Green
    Write-Host "  FAIL: $fails" -ForegroundColor $(if ($fails -gt 0) { "Red" } else { "Green" })

    if ($content -match "Auto-capture complete") {
        Write-Host "  Auto-capture: completed successfully" -ForegroundColor Green
    } else {
        Write-Host "  Auto-capture: NOT found in log (may not have reached gameplay)" -ForegroundColor Yellow
    }
}
Write-Host "  Logs: $logDir\" -ForegroundColor Gray
