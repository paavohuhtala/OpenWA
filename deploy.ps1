$dest = "I:\games\SteamLibrary\steamapps\common\Worms Armageddon"

cargo build --release -p openwa-wormkit
if ($LASTEXITCODE -ne 0) { exit 1 }

$src = "target\i686-pc-windows-msvc\release"
Copy-Item "$src\openwa_wormkit.dll" "$dest\wkOpenWA.dll"

Write-Host "Deployed to $dest"
