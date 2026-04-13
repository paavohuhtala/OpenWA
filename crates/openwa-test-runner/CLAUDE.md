# openwa-test-runner

Replay test runner (`openwa-test` binary). Supports headless (concurrent, log-diff) and headful (sequential, gameplay checks) modes.

## Subcommands

- `openwa-test [filter] [-j N] [-d DIR] [--no-build] [--wa-path PATH]` — Headless: discovers `*.WAgame`/`*.wagame` with `*_expected.log` in DIR (default `testdata/replays`), runs concurrently via `/getlog`, compares output byte-for-byte.
- `openwa-test headful [filter|replay.WAgame] [--no-build] [--wa-path PATH] [--timeout SECS]` — Headful: runs with graphics+sound, checks for crashes/panics/gameplay markers. Default timeout 150s.
- `openwa-test trace-desync <replay.WAgame> [--no-build] [--wa-path PATH]` — Compares per-frame checksums between baseline and hooked runs.
- `openwa-test generate-baseline [filter] [-d DIR] [-j N] [--no-build] [--wa-path PATH]` — Generates `*_expected.log` baselines for replays that don't have one yet. Runs with `OPENWA_TRACE_BASELINE=1` (minimal hooks).

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

## Headful Pass Criteria

A headful test passes when: no crash, no `[PANIC]` in OpenWA.log, no `[GAMEPLAY FAIL]`, and at least one `[GAMEPLAY PASS]` marker present.

## Adding New Replay Tests

1. Record a game in WA.exe (the replay `.WAgame` file is saved automatically)
2. Copy the replay to `testdata/replays/`
3. Run once with the headless test runner — it auto-generates the `*_expected.log` baseline
4. Subsequent runs compare against this baseline

## Environment Variables

- `OPENWA_HEADLESS=1` — Headless mode: hooks MessageBoxA to auto-dismiss, launcher uses SW_HIDE, file isolation hook active, semaphore renamed per-PID
- `OPENWA_REPLAY_TEST=1` — Headful mode: enables fast-forward replay (50x speed)
- `OPENWA_LOG_PATH=<path>` — Override OpenWA.log location (used for per-instance isolation)
- `OPENWA_ERRORLOG_PATH=<path>` — Redirect ERRORLOG.TXT to specified path (crash capture)

## Worms 2D Speedrun Suite

~598 replays in `testdata/replays/worms2d/` (`.wagame` extension). Supports high concurrency (`-j 16` or higher). Full suite takes ~40s at `-j 16`. Use for final validation after complex changes.

```bash
openwa-test -d testdata/replays/worms2d -j 16
```

## Key Paths

- Replay files + expected logs: `testdata/replays/*.WAgame` + `*_expected.log`
- Worms 2D replays: `testdata/replays/worms2d/*.wagame` + `*_expected.log`
- Per-run output: `testdata/runs/<timestamp>/` (headless), `testdata/runs/headful-<timestamp>/` (headful)
- Headless script: `run-tests.ps1`
- Headful script: `replay-test.ps1`
