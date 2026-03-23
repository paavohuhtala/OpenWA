# Run all headless replay integration tests.
# Usage: .\run-tests.ps1 [filter] [-j N]
# Examples:
#   .\run-tests.ps1              # all tests, default parallelism
#   .\run-tests.ps1 longbow      # only tests matching "longbow"
#   .\run-tests.ps1 -j 1         # serial mode

# Build the test runner first to ensure it's up-to-date
cargo build -p openwa-test-runner --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& "$PSScriptRoot\target\i686-pc-windows-msvc\release\openwa-test.exe" @args
