# start.ps1 - Build and launch WA.exe
$ErrorActionPreference = "Stop"

Write-Host "Building..." -ForegroundColor Cyan
cargo build --release -p openwa-launcher -p openwa-wormkit
if ($LASTEXITCODE -ne 0) { exit 1 }

& "target\i686-pc-windows-msvc\release\openwa-launcher.exe"
