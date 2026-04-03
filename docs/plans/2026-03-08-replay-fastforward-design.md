# Replay Fast-Forward Design

## Goal

Automatically fast-forward through entire WA replays during `OPENWA_REPLAY_TEST=1` mode, enabling comprehensive testing beyond the current 5-second window.

## Background

The current replay test captures only 5 seconds of gameplay — often not enough for a single worm turn. WA advances one turn per spacebar press during replay playback. The game detects key-DOWN transitions (not held state) and has no key repeat, so spacebar must be repeatedly pressed and released.

WA.exe exits automatically when a replay finishes, so no explicit exit is needed if we can fast-forward through the whole thing.

## Approach: Hook GetAsyncKeyState

Hook `GetAsyncKeyState` from `user32.dll` via MinHook. When the game queries `VK_SPACE` and fast-forward mode is active, return simulated key-down/key-up transitions.

**Why this over keybd_event/SendInput:**
- Works when WA.exe is minimized (no window focus required)
- Intercepts at the API level regardless of how WA reads the result
- No timing races with the game's message pump
- Existing MinHook infrastructure

### Key simulation

GetAsyncKeyState returns a SHORT:
- Bit 15 (0x8000) = key currently down
- Bit 0 (0x0001) = key pressed since last call

Toggle between calls:
- Even: `0x8001` (down + transition)
- Odd: `0x0000` (up, no transition)

All other virtual keys pass through to the real function.

### Activation

1. DLL loads, detects `OPENWA_REPLAY_TEST=1`
2. Spawns thread that waits ~3 seconds for game to reach gameplay
3. Sets atomic flag to enable fast-forward
4. GetAsyncKeyState hook checks flag on each VK_SPACE query

### Validation integration

Current: thread waits 5s -> validate -> ExitProcess.

New: thread waits 3s -> enable fast-forward -> wait up to 120s for natural exit. Validation dumps trigger on a timeout (60s) as a safety net. Normally the game exits on its own when the replay finishes.

### Script changes

`replay-test.ps1`: add timeout to `WaitForExit()` (120s). Remove the implicit 5s assumption from comments.

## Future: Turn-Start Hook

Hooking the function that runs at each turn start would enable per-turn assertions and proper unit-test-style validation during replay playback. This is out of scope for this design but is the natural next step.

## Files

- `crates/openwa-dll/src/replacements/input.rs` (new) — GetAsyncKeyState hook
- `crates/openwa-dll/src/validation/mod.rs` — Update auto-capture timeout
- `crates/openwa-dll/src/lib.rs` — Register input module
- `replay-test.ps1` — Add WaitForExit timeout
