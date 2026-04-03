---
name: desync-debug
description: Diagnose replay desync failures using trace-desync, hardware watchpoints, and per-frame analysis. Use ONLY after a headless replay test has already detected a checksum mismatch.
---

# Desync Debugging

**This is a diagnostic workflow, not a testing tool.** Only use after a headless replay test has already failed with a log mismatch. Do NOT run trace-desync speculatively or as part of normal testing.

Workflow: test fails -> trace-desync to find divergent frame -> investigate cause.

## Trace-Desync

The `trace-desync` subcommand hooks WA's own `GameFrameChecksumProcessor` (0x5329C0) to capture per-frame checksums, runs the game twice (baseline with minimal hooks vs all hooks), and diffs the results:

```bash
.\trace-desync.ps1 testdata/replays/longbow.WAgame

# Or directly:
openwa-test trace-desync testdata/replays/longbow.WAgame [--no-build] [--wa-path PATH]
```

Baseline mode (`OPENWA_TRACE_BASELINE=1`) installs only: headless, file_isolation, frame_hook, trace_desync. All gameplay hooks (replay, weapon, sound, constructor, etc.) are skipped, giving a "nearly vanilla" WA reference run.

Output reports the first divergent frame or confirms all checksums match. Per-frame hash logs are saved in `testdata/runs/trace-<timestamp>/`.

## Desync Debugging Methodology

Replay desyncs (checksum mismatches) can be caused by any code difference -- constructor side effects, hooked function behaviour, missing state, wrong calling conventions, etc. Key methodology:

0. **Start with `trace-desync`**: Run `.\trace-desync.ps1 testdata/replays/<replay>.WAgame` to automatically find the exact frame where baseline and hooked runs diverge.
1. **WA uses a single shared RNG** (DDGame+0x45EC, `AdvanceGameRNG` at 0x53F320) for both gameplay AND visual effects. There is no separate "visual RNG." Even purely decorative things like particle sprites affect the game RNG and will cause desyncs in headless mode if handled differently. A secondary effect RNG exists at DDGame+0x45F0 (`advance_effect_rng()`, simpler LCG without frame_counter) -- used by WeaponRelease visual effects. Uses `team_health_ratio[0]` (unused index-0 slot).
2. **DDGame flat memory matching is NOT sufficient.** Constructors and hooks have side effects on sub-objects (display, GfxHandler, PCLandscape). Compare all objects pointed to by DDGame AND DDGameWrapper.
3. **Use hardware watchpoints** (see "Hardware Watchpoints" in CLAUDE.md) to find what writes a specific field.
4. **Per-frame RNG logging** (DDGame+0x45EC) pinpoints the exact frame where simulation diverges. Binary search on frames, not code.
5. **Always validate diff methodology** against a known-good frame first. The snapshot system's pointer canonicalization produces false positives.

See `docs/re-notes/desync-investigation.md` for a detailed case study.

## Environment Variables

- `OPENWA_TRACE_DESYNC=1` -- Enable per-frame checksum logging (hooks GameFrameChecksumProcessor)
- `OPENWA_TRACE_BASELINE=1` -- Baseline mode: skip all gameplay hooks, keep only infrastructure
- `OPENWA_TRACE_HASH_PATH=<path>` -- Override frame hash log location (default: frame_hashes.log)

See "Hardware Watchpoints" in CLAUDE.md for watchpoint env vars (`OPENWA_WATCH_FRAME`, etc.).

## Key Files

- `crates/openwa-dll/src/replacements/trace_desync.rs` -- Per-frame checksum logging
- `crates/openwa-dll/src/debug_watchpoint.rs` -- Hardware watchpoint instrumentation
- `crates/openwa-dll/src/replacements/frame_hook.rs` -- Frame hook (watchpoint arming)
- `crates/openwa-test-runner/` -- Test runner that detects failures
- `docs/re-notes/desync-investigation.md` -- Detailed case study
- `trace-desync.ps1` -- Convenience script
