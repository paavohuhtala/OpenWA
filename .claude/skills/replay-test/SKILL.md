---
name: replay-test
description: Run automated replay-based testing - builds, launches WA.exe with a replay file, captures validation logs, and presents results. Supports headful (graphics + validation) and headless (pure simulation + log diff) modes.
---

# Replay Test

Run the replay-based automated testing pipeline.

## Quick Start — Run All Tests

The fastest way to run all replay tests:

```bash
.\run-tests.ps1
```

This builds and runs the `openwa-test` binary via `cargo run`, which then builds the DLL + launcher, discovers all `testdata/replays/*.WAgame` files with matching `*_expected.log`, and runs them concurrently (default 4 workers) in headless mode. Output:

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

## Modes

### Headless test runner (primary)

Uses WA's built-in `/getlog` mode: no rendering, pure CPU simulation. Compares the output log byte-for-byte against an expected baseline.

```bash
# All tests:
.\run-tests.ps1

# Single replay via old script:
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless testdata/replays/longbow.WAgame
```

### Headful (interactive) — graphics, sound, validation

Tests hooks, struct layouts, and game state with the full rendering pipeline active. Use for debugging specific replays.

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

## Steps (for Claude)

1. Run the appropriate test command.

2. Read the output and present results:

**Headless (test runner):**
- PASS/FAIL summary is printed directly
- Per-run logs stored in `testdata/runs/<timestamp>/`
- FAIL shows first differing lines between expected and actual

**Headless (single replay via script):**
- PASS means byte-identical output to `<replay>_expected.log`
- FAIL shows the diff lines

**Headful mode:**
- If `testdata/logs/errorlog_latest.txt` exists, WA.exe crashed. Read it and report the crash
- Read `testdata/logs/validation_latest.log`:
  - **Static checks** (`[STATIC PASS]`/`[STATIC FAIL]`): vtable addresses, struct offsets
  - **Gameplay checks** (`[GAMEPLAY PASS]`/`[GAMEPLAY FAIL]`): game init, match start, match completion
- Quote any FAIL lines exactly
- Check `testdata/logs/openwa_latest.log` for `[PANIC]` errors

## Environment Variables

- `OPENWA_HEADLESS=1` — Headless mode: auto-dismiss MessageBoxA, SW_HIDE window, file isolation
- `OPENWA_REPLAY_TEST=1` — (headful) Enable fast-forward mode (50x speed)
- `OPENWA_LOG_PATH=<path>` — Override OpenWA.log location (per-instance isolation)

All are set automatically by the test runner / replay-test.ps1.

## Key Paths

- Replay files + expected logs: `testdata/replays/*.WAgame` + `*_expected.log`
- Per-run output: `testdata/runs/<timestamp>/` (gitignored)
- Test runner: `crates/openwa-test-runner/` (`openwa-test` binary)
- Convenience script: `run-tests.ps1`
- Single-replay script: `replay-test.ps1`

## Adding New Replay Tests

1. Record a game in WA.exe (replay `.WAgame` saved automatically)
2. Copy to `testdata/replays/`
3. Run `.\run-tests.ps1` — auto-generates `*_expected.log` on first run
4. Subsequent runs compare against the baseline

## Notes

- Headful replay: ~15-30s; headless: ~5s per test
- Concurrent headless tests use per-PID event names, log paths, and temp files (CreateFileA hook)
- Named event: `OpenWA_HooksReady_{pid}` — ensures hooks installed before WA main thread runs
