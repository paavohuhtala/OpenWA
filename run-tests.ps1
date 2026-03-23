# Run all headless replay integration tests.
# Usage: .\run-tests.ps1 [filter] [-j N]
# Examples:
#   .\run-tests.ps1              # all tests, default parallelism
#   .\run-tests.ps1 longbow      # only tests matching "longbow"
#   .\run-tests.ps1 -j 1         # serial mode

cargo run -p openwa-test-runner --release -- @args
