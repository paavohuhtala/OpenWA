# Replay-Based Automated Testing Design

**Date:** 2026-03-07
**Status:** Approved

## Problem

The current validation workflow is manual: start WA.exe, navigate menus, start a game, press F9/F10 hotkeys, read logs. This is slow and non-reproducible. We need an automated way to launch the game into a known gameplay state for validation and RE discovery.

## Key Insight

WA.exe accepts a replay file (`.WAgame`) as a command-line argument. It starts playing automatically, skipping menus, and closes when the replay finishes. This gives us a deterministic, automated path into gameplay state.

## Design

### Components

#### 1. PowerShell script (`replay-test.ps1`)

Orchestrates the full workflow:

1. Build both DLLs (`cargo build --release -p openwa-wormkit -p openwa-validator`)
2. Deploy to game directory (same as `deploy.ps1`)
3. Clear old `OpenWA_validation.log` and `OpenWA.log` in game directory
4. Set `OPENWA_REPLAY_TEST=1` in environment
5. Launch `WA.exe <replay-file>` and wait for process exit
6. Copy both log files to `testdata/logs/` in the project
7. Print summary (PASS/FAIL counts from log)

Accepts one argument: replay file path. Defaults to `testdata/replays/bots.WAgame`.

#### 2. Validator DLL changes (`openwa-validator`)

At startup in `run_validation()`, check `std::env::var("OPENWA_REPLAY_TEST")`:

- **Not set:** Current behavior (deferred timer threads, hotkey listener, runs indefinitely).
- **Set:** Spawn a single "auto-capture" thread that:
  1. Waits 30 seconds (enough for replay to reach gameplay)
  2. Runs `deferred_global_validation()`
  3. Runs `dump_team_blocks()`
  4. Runs `dump_landscape()`
  5. Logs `"--- Auto-capture complete, exiting ---"`
  6. Calls `ExitProcess(0)` to terminate WA.exe

No hotkey listener or timer threads in automated mode -- one clean capture sequence.

#### 3. Claude Code skill (`replay-test`)

A skill that:
- Invokes `replay-test.ps1` via Bash
- Reads the resulting log files from `testdata/logs/`
- Presents results in the conversation

### Data Flow

```
/replay-test
  -> Bash: powershell -File replay-test.ps1 testdata/replays/bots.WAgame
    -> cargo build --release
    -> deploy DLLs to game dir
    -> clear logs
    -> set OPENWA_REPLAY_TEST=1
    -> start WA.exe bots.WAgame (blocks until exit)
      -> validator DLL starts
      -> detects env var -> auto-capture mode
      -> waits 30s -> dumps -> ExitProcess(0)
    -> WA.exe exits
    -> copy logs to testdata/logs/
    -> print summary
  -> Skill reads testdata/logs/*.log
  -> presents to user
```

### File Locations

| File | Location |
|------|----------|
| Orchestration script | `replay-test.ps1` (project root) |
| Replay files | `testdata/replays/*.WAgame` |
| Captured logs | `testdata/logs/` (gitignored) |
| Skill definition | User's Claude Code skills directory |

### Environment Variable

| Variable | Value | Effect |
|----------|-------|--------|
| `OPENWA_REPLAY_TEST` | `1` | Validator enters auto-capture mode: single 30s wait, dump all, ExitProcess |
| (unset) | -- | Normal interactive mode with hotkeys and timer threads |

## Future Expansion (Not In Scope)

- Configurable wait time or dump selection via env vars or config file
- Event-based hooks (dump on CreateExplosion, WeaponRelease, etc.)
- Frame-count triggers via ProcessFrame hook
- Structured output (JSON) for programmatic diffing
- Multiple replay files in one run
- CI integration with exit codes based on PASS/FAIL counts
