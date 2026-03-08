---
name: replay-test
description: Run automated replay-based testing - builds, deploys, launches WA.exe with a replay file, captures validation logs, and presents results. Use when you need to validate struct layouts, hooks, or game state against live WA.exe.
---

# Replay Test

Run the replay-based automated testing pipeline.

## Steps

1. Run the replay test script:

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

This will:
- Build the unified openwa-wormkit DLL (includes validation)
- Deploy wkOpenWA.dll to the WA game directory
- Set `OPENWA_VALIDATE=1` and `OPENWA_REPLAY_TEST=1` env vars
- Launch WA.exe minimized with the default replay file (testdata/replays/bots.WAgame)
- The DLL restores the window after 2s via FindWindowA + ShowWindow(SW_RESTORE)
- Fast-forward through the entire replay by setting DDGame+0x98B0 (50 frames per render cycle)
- Validation runs at the 5s mark, game exits when replay finishes (120s safety timeout)
- Copy logs to testdata/logs/

2. Read the validation log and present results:

Read `testdata/logs/validation_latest.log` and summarize:
- Total PASS/FAIL counts
- Any [FAIL] lines (quote them exactly)
- Whether validation completed or safety timeout triggered
- Key data from dumps (team blocks, landscape) if present

3. If the user also has `testdata/logs/openwa_latest.log`, read it and note any errors or interesting hook activity.

## Environment Variables

- `OPENWA_VALIDATE=1` — Enables the validation module (struct checks, vtable validation, memory dumps). Without this, only the replacement hooks run.
- `OPENWA_REPLAY_TEST=1` — Enables fast-forward mode: hooks TurnManager_ProcessFrame and sets DDGame+0x98B0=1 each frame, causing the game engine to process up to 50 frames per render cycle. The DLL auto-restores the window at 2s, runs validation at 5s, and has a 120s safety timeout. Without this, validation runs in interactive mode with hotkeys (F9=team blocks, F10=landscape).

Both are set automatically by `replay-test.ps1`.

## Notes

- The fast-forwarded replay typically completes in ~15-30s
- The DLL restores the minimized window automatically (steals focus briefly)
- There is only one DLL now: `wkOpenWA.dll` (unified wormkit + validator)
- The old `wkOpenWAValidator.dll` is automatically removed by the script
- Replay file can be changed: `powershell -File replay-test.ps1 path\to\other.WAgame`
- Occasional crashes may occur during fast-forward (null vtable calls from 50x speed processing); retry if this happens
