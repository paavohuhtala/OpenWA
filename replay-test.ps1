# Run a headful replay test via the Rust test runner.
# Usage: .\replay-test.ps1 [filter|replay.WAgame] [--no-build] [--wa-path PATH]
cargo build -p openwa-test-runner --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& "$PSScriptRoot\target\i686-pc-windows-msvc\release\openwa-test.exe" headful @args
