# Run trace-desync: compare per-frame checksums between baseline and hooked WA.
# Usage: .\trace-desync.ps1 <replay.WAgame> [--no-build] [--wa-path PATH]
# Examples:
#   .\trace-desync.ps1 testdata/replays/longbow.WAgame
#   .\trace-desync.ps1 testdata/replays/longbow.WAgame --no-build

cargo build -p openwa-test-runner --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& "$PSScriptRoot\target\i686-pc-windows-msvc\release\openwa-test.exe" trace-desync --no-build @args
