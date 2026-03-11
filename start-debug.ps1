# start-debug.ps1 - Build and launch WA.exe with the debug UI overlay
$ErrorActionPreference = "Stop"

Write-Host "Building..." -ForegroundColor Cyan
cargo build --release -p openwa-launcher -p openwa-wormkit --features openwa-wormkit/debug-ui
if ($LASTEXITCODE -ne 0) { exit 1 }

$env:OPENWA_DEBUG_UI = "1"
& "target\i686-pc-windows-msvc\release\openwa-launcher.exe"
