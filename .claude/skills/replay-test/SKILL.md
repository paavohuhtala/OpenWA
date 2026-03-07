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
- Build openwa-wormkit and openwa-validator
- Deploy DLLs to the WA game directory
- Launch WA.exe with the default replay file (testdata/replays/bots.WAgame)
- Wait for auto-capture (5s) and process exit
- Copy logs to testdata/logs/

2. Read the validation log and present results:

Read `testdata/logs/validation_latest.log` and summarize:
- Total PASS/FAIL counts
- Any [FAIL] lines (quote them exactly)
- Whether auto-capture completed
- Key data from dumps (team blocks, landscape) if present

3. If the user also has `testdata/logs/openwa_latest.log`, read it and note any errors or interesting hook activity.

## Notes

- The script takes ~10 seconds total (build + 5s auto-capture wait)
- If WA.exe doesn't exit, the validator DLL may not have loaded. Check that wkOpenWAValidator.dll exists in the game directory.
- Replay file can be changed by editing the script invocation: `powershell -File replay-test.ps1 path\to\other.WAgame`
