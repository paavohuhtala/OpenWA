# openwa-test-runner

Headless replay test runner (`openwa-test` binary). Discovers replay tests in `testdata/replays/`, runs them concurrently via WA.exe's `/getlog` mode, and compares output logs byte-for-byte against expected baselines.

## Test Isolation

All isolation mechanisms are active in headless mode only:

- Per-PID temp directory: `.openwa_tmp/{pid}/` for writable files (writetest.txt, mono.tmp, land.dat, landgen.svg, etc.)
- Per-PID named event: `OpenWA_HooksReady_{pid}` for launcher-DLL synchronization
- Per-PID semaphore: `CreateSemaphoreA("Worms Armageddon")` renamed to `Worms Armageddon_{pid}`
- Per-PID log paths via `OPENWA_LOG_PATH` and `OPENWA_ERRORLOG_PATH` env vars
- Per-PID file redirection via `file_isolation.rs` (CreateFileA hook redirects playback.thm, current.thm, land.dat, landgen.svg to `.openwa_tmp/{pid}/`)
- WA.exe crash dialog suppressed via `/silentcrash` command-line flag
- Batched MinHook enables: all hooks use `queue_enable_hook` + single `apply_queued()` call

## Crash Detection

Tests that crash show `CRASH` (not `FAIL`) with NTSTATUS name and ERRORLOG.TXT content. ERRORLOG.TXT is redirected to the per-test run directory via `OPENWA_ERRORLOG_PATH`.

## Adding New Replay Tests

1. Record a game in WA.exe (the replay `.WAgame` file is saved automatically)
2. Copy the replay to `testdata/replays/`
3. Run once with the headless test runner — it auto-generates the `*_expected.log` baseline
4. Subsequent runs compare against this baseline

## Environment Variables

- `OPENWA_HEADLESS=1` — Headless mode: hooks MessageBoxA to auto-dismiss, launcher uses SW_HIDE, file isolation hook active, semaphore renamed per-PID
- `OPENWA_LOG_PATH=<path>` — Override OpenWA.log location (used for per-instance isolation)
- `OPENWA_ERRORLOG_PATH=<path>` — Redirect ERRORLOG.TXT to specified path (crash capture)

## Key Paths

- Replay files + expected logs: `testdata/replays/*.WAgame` + `*_expected.log`
- Per-run output: `testdata/runs/<timestamp>/` (gitignored)
- Convenience script: `run-tests.ps1`
