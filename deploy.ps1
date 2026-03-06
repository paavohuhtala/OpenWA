$dest = "I:\games\SteamLibrary\steamapps\common\Worms Armageddon"

cargo build --release -p openwa-wormkit -p openwa-validator
if ($LASTEXITCODE -ne 0) { exit 1 }

$src = "target\i686-pc-windows-msvc\release"
Copy-Item "$src\openwa_wormkit.dll" "$dest\wkOpenWA.dll"
Copy-Item "$src\openwa_validator.dll" "$dest\wkOpenWAValidator.dll"

Write-Host "Deployed to $dest"
