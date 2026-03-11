$dest = "I:\games\SteamLibrary\steamapps\common\Worms Armageddon"

cargo build --release -p openwa-wormkit --features debug-ui
if ($LASTEXITCODE -ne 0) { exit 1 }

$src = "target\i686-pc-windows-msvc\release"
Copy-Item "$src\openwa_wormkit.dll" "$dest\wkOpenWA.dll"

Write-Host "Deployed debug-ui build to $dest"
Write-Host "Launch WA.exe with OPENWA_DEBUG_UI=1 to show the debug window."
Write-Host ""
Write-Host "Example:"
Write-Host '  $env:OPENWA_DEBUG_UI=1; & "I:\games\SteamLibrary\steamapps\common\Worms Armageddon\WA.exe"'
