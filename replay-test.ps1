# replay-test.ps1 - Automated replay-based testing
# Usage: powershell -File replay-test.ps1 [-Headless] [replay-file]
#
# Modes:
#   Default (headful): Fast-forward replay with validation, graphics & sound active.
#   -Headless:         Uses WA's /getlog mode -- no window, pure simulation.
#                      Compares output log to expected log for determinism checking.

param(
    [switch]$Headless,
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
Remove-Item "$gameDir\ERRORLOG.TXT"          -ErrorAction SilentlyContinue

# Remove old WormKit DLL if present from a previous deployment
if (Test-Path "$gameDir\wkOpenWA.dll") {
    Write-Host "Note: removing legacy $gameDir\wkOpenWA.dll" -ForegroundColor Yellow
    Remove-Item "$gameDir\wkOpenWA.dll"
}
if (Test-Path "$gameDir\wkOpenWAValidator.dll") {
    Remove-Item "$gameDir\wkOpenWAValidator.dll"
}

# 3. Launch via launcher with replay, env vars set
$env:OPENWA_WA_PATH = $waExe

if ($Headless) {
    Write-Host "Launching WA.exe in HEADLESS mode: $ReplayFile" -ForegroundColor Cyan
    Write-Host "  (No window -- pure simulation via /getlog)" -ForegroundColor Yellow

    # Remove any previous output log next to the replay file
    $replayBase = [System.IO.Path]::ChangeExtension($ReplayFile, ".log")
    Remove-Item $replayBase -ErrorAction SilentlyContinue

    $env:OPENWA_HEADLESS = "1"
    $proc = Start-Process -FilePath $launcher -ArgumentList "`"$waExe`" /getlog `"$ReplayFile`"" -PassThru
    $timeout = 120
} else {
    Write-Host "Launching WA.exe with replay: $ReplayFile" -ForegroundColor Cyan
    Write-Host "  (Fast-forward mode: replay will be auto-advanced, game exits when done)" -ForegroundColor Yellow

    $env:OPENWA_VALIDATE    = "1"
    $env:OPENWA_REPLAY_TEST = "1"
    $proc = Start-Process -FilePath $launcher -ArgumentList "--minimized `"$waExe`" `"$ReplayFile`"" -PassThru
    $timeout = 150
}

if (-not $proc.WaitForExit($timeout * 1000)) {
    Write-Host "WARNING: launcher did not exit within ${timeout}s, killing..." -ForegroundColor Red
    $proc.Kill()
    $proc.WaitForExit(5000)
}
$env:OPENWA_VALIDATE    = $null
$env:OPENWA_REPLAY_TEST = $null
$env:OPENWA_HEADLESS    = $null
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

$crashed = $false
if (Test-Path "$gameDir\ERRORLOG.TXT") {
    Copy-Item "$gameDir\ERRORLOG.TXT" "$logDir\errorlog_$timestamp.txt"
    Copy-Item "$gameDir\ERRORLOG.TXT" "$logDir\errorlog_latest.txt"
    Write-Host "ERRORLOG.TXT copied (crash detected)." -ForegroundColor Red
    $crashed = $true
} else {
    Remove-Item "$logDir\errorlog_latest.txt" -ErrorAction SilentlyContinue
}

# 5. Crash check (before summary, so it always runs)
if ($crashed) {
    Write-Host ""
    Write-Host "=== CRASH DETECTED ===" -ForegroundColor Red
    Get-Content "$logDir\errorlog_latest.txt" -TotalCount 5 | ForEach-Object {
        Write-Host "  $_" -ForegroundColor Red
    }
    Write-Host "  ..." -ForegroundColor Red
    Write-Host "  Full log: $logDir\errorlog_latest.txt" -ForegroundColor Gray
    exit 1
}

# 6. Print summary
Write-Host ""
if ($Headless) {
    Write-Host "=== Headless Log Comparison ===" -ForegroundColor Cyan
    $replayBase = [System.IO.Path]::ChangeExtension($ReplayFile, ".log")
    $expectedLog = [System.IO.Path]::Combine(
        [System.IO.Path]::GetDirectoryName($ReplayFile),
        [System.IO.Path]::GetFileNameWithoutExtension($ReplayFile) + "_expected.log"
    )

    if (-not (Test-Path $replayBase)) {
        Write-Host "  FAIL: No output log generated at $replayBase" -ForegroundColor Red
        exit 1
    }
    if (-not (Test-Path $expectedLog)) {
        Write-Host "  No expected log found -- saving output as expected: $expectedLog" -ForegroundColor Yellow
        Copy-Item $replayBase $expectedLog
        Write-Host "  PASS (first run -- expected log created)" -ForegroundColor Green
    } else {
        $diff = Compare-Object (Get-Content $expectedLog) (Get-Content $replayBase)
        if ($diff) {
            Write-Host "  FAIL: Output differs from expected log" -ForegroundColor Red
            Write-Host ""
            $diff | ForEach-Object {
                $indicator = if ($_.SideIndicator -eq "<=") { "expected" } else { "actual  " }
                Write-Host "  $indicator | $($_.InputObject)" -ForegroundColor $(if ($_.SideIndicator -eq "<=") { "Red" } else { "Yellow" })
            }
            exit 1
        } else {
            Write-Host "  PASS: Output matches expected log" -ForegroundColor Green
        }
    }
    # Clean up the generated log
    Remove-Item $replayBase -ErrorAction SilentlyContinue
} else {
    Write-Host "=== Validation Summary ===" -ForegroundColor Cyan
    if (Test-Path "$logDir\validation_latest.log") {
        $content = Get-Content "$logDir\validation_latest.log" -Raw

        # Static checks (vtables, struct offsets, prologues)
        $staticPasses = ([regex]::Matches($content, "\[STATIC PASS\]")).Count
        $staticFails  = ([regex]::Matches($content, "\[STATIC FAIL\]")).Count
        $staticColor = if ($staticFails -gt 0) { "Red" } else { "Green" }
        Write-Host "  Static checks:   $staticPasses pass, $staticFails fail  (vtables, struct offsets, prologues)" -ForegroundColor $staticColor

        # Gameplay checks (milestones from frame hook)
        $gameplayPasses = ([regex]::Matches($content, "\[GAMEPLAY PASS\]")).Count
        $gameplayFails  = ([regex]::Matches($content, "\[GAMEPLAY FAIL\]")).Count
        if ($gameplayPasses + $gameplayFails -gt 0) {
            $gameplayColor = if ($gameplayFails -gt 0) { "Red" } else { "Green" }
            Write-Host "  Gameplay checks: $gameplayPasses pass, $gameplayFails fail  (init, match start, match end)" -ForegroundColor $gameplayColor
        } else {
            Write-Host "  Gameplay checks: not found (DLL may not have detached cleanly)" -ForegroundColor Yellow
        }

        if ($content -match "Safety timeout reached") {
            Write-Host "  Replay: TIMEOUT (safety timeout triggered)" -ForegroundColor Red
        }
    }
    # Check for Rust panics in OpenWA log
    if (Test-Path "$logDir\openwa_latest.log") {
        $panics = Select-String -Path "$logDir\openwa_latest.log" -Pattern "\[PANIC\]"
        if ($panics) {
            Write-Host ""
            Write-Host "=== RUST PANIC DETECTED ===" -ForegroundColor Red
            $panics | ForEach-Object { Write-Host "  $($_.Line)" -ForegroundColor Red }
        }
    }

    Write-Host "  Logs: $logDir\" -ForegroundColor Gray
}
