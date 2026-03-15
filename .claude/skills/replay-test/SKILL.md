---
name: replay-test
description: Run automated replay-based testing - builds, launches WA.exe with a replay file, captures validation logs, and presents results. Supports headful (graphics + validation) and headless (pure simulation + log diff) modes.
---

# Replay Test

Run the replay-based automated testing pipeline.

## Modes

### Headful (default) — graphics, sound, validation
Tests hooks, struct layouts, and game state with the full rendering pipeline active.

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

### Headless — pure simulation, log comparison
Uses WA's built-in `/getlog` mode: no window, no rendering, pure CPU simulation. Compares the output log byte-for-byte against an expected log to verify replay determinism. An order of magnitude faster than headful mode.

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

## Steps

1. Run the replay test script (headful or headless as appropriate).

2. Read the output and present results:

**Both modes:**
- If `testdata/logs/errorlog_latest.txt` exists, WA.exe crashed. Read it and report the crash (exception type, address, registers)

**Headful mode:**
- Read `testdata/logs/validation_latest.log` and summarize total PASS/FAIL counts
- Quote any [FAIL] lines exactly
- Note whether validation completed or safety timeout triggered
- If `testdata/logs/openwa_latest.log` exists, check for errors or interesting hook activity

**Headless mode:**
- The script compares the output log to `testdata/replays/bots_expected.log`
- PASS means byte-identical output -- replay is deterministic
- FAIL shows the diff lines between expected and actual

## Environment Variables

- `OPENWA_VALIDATE=1` — (headful) Enables validation module
- `OPENWA_REPLAY_TEST=1` — (headful) Enables fast-forward mode (50x speed)
- `OPENWA_HEADLESS=1` — (headless) Hooks MessageBoxA to auto-dismiss, enabling unattended /getlog

All are set automatically by `replay-test.ps1`.

## Notes

- Headful replay typically completes in ~15-30s; headless in ~5s
- Custom replay: `powershell -File replay-test.ps1 [-Headless] path\to\other.WAgame`
- Expected log convention: `<replay>_expected.log` next to `<replay>.WAgame`
- If no expected log exists, headless mode saves the first output as the expected log
- The launcher uses named event synchronization to ensure all hooks are installed before WA.exe's main thread runs
