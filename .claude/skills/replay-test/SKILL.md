---
name: replay-test
description: Run automated replay-based testing - builds, launches WA.exe with a replay file, and presents results. Supports headful (graphics + gameplay checks) and headless (pure simulation + log diff) modes via the openwa-test binary.
---

# Replay Test

Run the replay-based automated testing pipeline.

## Quick Start — Run All Tests (Headless)

The fastest way to run all replay tests:

```bash
.\run-tests.ps1
```

This builds and runs the `openwa-test` binary, which builds the DLL + launcher, discovers all `testdata/replays/*.WAgame` files with matching `*_expected.log`, and runs them concurrently (default 4 workers) in headless mode. Output:

```
Building... done (2.1s)
Running 7 tests (4 concurrent)...

  PASS  bots                    (5.2s)
  PASS  longbow                 (4.9s)
  ...

7 tests: all passed (wall 12.0s, cpu 40.3s)
```

Options:
- `.\run-tests.ps1 longbow` — filter by name
- `.\run-tests.ps1 -j 1` — serial mode (for debugging)
- `.\run-tests.ps1 --no-build` — skip internal DLL/launcher build (assumes already built)
- `.\run-tests.ps1 -d testdata/replays/worms2d` — run the Worms 2D speedrun replay suite instead

## Modes

### Headless (primary)

Uses WA's built-in `/getlog` mode: no rendering, pure CPU simulation. Compares the output log byte-for-byte against an expected baseline.

```bash
.\run-tests.ps1
.\run-tests.ps1 longbow
```

### Headful — graphics, sound, gameplay validation

Tests hooks, struct layouts, and game state with the full rendering pipeline active. Runs sequentially. The game window must be focused once for the replay to start advancing, after which it runs to completion on its own.

```bash
.\replay-test.ps1
.\replay-test.ps1 bots
.\replay-test.ps1 testdata/replays/bots.WAgame
.\replay-test.ps1 bots --timeout 600    # 10 minute timeout
```

Both scripts are thin wrappers around `openwa-test` / `openwa-test headful`.

## Steps (for Claude)

1. Run the appropriate test command.

2. Read the output and present results:

**Headless:**
- PASS/FAIL summary is printed directly
- Per-run logs stored in `testdata/runs/<timestamp>/`
- FAIL shows first differing lines between expected and actual

**Headful:**
- PASS/FAIL summary is printed directly
- Per-run logs stored in `testdata/runs/headful-<timestamp>/`
- Checks OpenWA.log for `[PANIC]` errors and `[GAMEPLAY PASS]`/`[GAMEPLAY FAIL]` markers
- PASS requires: no crash, no panics, no gameplay fails, at least one gameplay pass
- CRASH shows NTSTATUS name and ERRORLOG.TXT content

## Environment Variables

- `OPENWA_HEADLESS=1` — Headless mode: auto-dismiss MessageBoxA, SW_HIDE window, file isolation
- `OPENWA_REPLAY_TEST=1` — (headful) Enable fast-forward mode (50x speed)
- `OPENWA_LOG_PATH=<path>` — Override OpenWA.log location (per-instance isolation)

All are set automatically by the test runner.

## Key Paths

- Replay files + expected logs: `testdata/replays/*.WAgame` + `*_expected.log`
- Worms 2D speedrun replays: `testdata/replays/worms2d/*.wagame` + `*_expected.log`
- Per-run output: `testdata/runs/<timestamp>/` (gitignored)
- Test runner: `crates/openwa-test-runner/` (`openwa-test` binary)
- Headless script: `run-tests.ps1`
- Headful script: `replay-test.ps1`

## Worms 2D Speedrun Replays

~598 replays from the [Worms 2D file archive](https://worms2d.info/files/replays/) in `testdata/replays/worms2d/`. These cover all 33 single-player campaign missions with diverse weapon usage, schemes, and game states.

```bash
# Run the Worms 2D suite (use -j 1 or -j 2 to avoid concurrency flakes)
openwa-test -d testdata/replays/worms2d -j 1
```

**Important:** At `-j 4` or higher, ~5-7% of tests flake with "Cannot play game file" errors due to WA.exe instances contending on shared resources not covered by file isolation hooks. Always use `-j 1` or `-j 2` for this suite. The full suite takes ~3-4 minutes at `-j 1`.

**When to run:** The full Worms 2D suite is slow — use it only as final validation after complex features or refactoring, not for iterative development. The base `testdata/replays/` suite (15 tests, ~1-2s) should be run routinely.

## Generating Baselines

Use the `generate-baseline` subcommand to create `_expected.log` files for new replays:

```bash
# Generate baselines for all replays in a directory that don't have one yet
openwa-test generate-baseline -d testdata/replays/worms2d -j 1

# With a filter
openwa-test generate-baseline -d testdata/replays/worms2d "wa01"
```

This runs each replay with `OPENWA_TRACE_BASELINE=1` (minimal hooks — only headless, file isolation, frame counting) and copies the output log as the expected baseline.

## Adding New Replay Tests

1. Record a game in WA.exe (replay `.WAgame` saved automatically)
2. Copy to `testdata/replays/`
3. Run `.\run-tests.ps1` — auto-generates `*_expected.log` on first run
4. Subsequent runs compare against the baseline

## Notes

- Headful replay: ~15-30s per test; headless: ~5s per test
- Headful timeout defaults to 150s, configurable with `--timeout SECS`
- Concurrent headless tests use per-PID event names, log paths, and temp files (CreateFileA hook)
- Named event: `OpenWA_HooksReady_{pid}` — ensures hooks installed before WA main thread runs
