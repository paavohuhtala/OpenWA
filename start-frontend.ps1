# start-frontend.ps1 - Build and launch WA.exe with the custom egui match-launcher window
$ErrorActionPreference = "Stop"

Write-Host "Building..." -ForegroundColor Cyan
cargo build --release -p openwa-launcher -p openwa-dll --features openwa-dll/match-launcher
if ($LASTEXITCODE -ne 0) { exit 1 }

$env:OPENWA_FRONTEND = "1"
& "target\i686-pc-windows-msvc\release\openwa-launcher.exe"
