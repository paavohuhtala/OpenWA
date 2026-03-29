# Frontend__MainNavigationLoop (0x4E6700)

Ghidra address: `0x004E6700`
Calling convention: `__fastcall` (ECX = CWinApp*)
Size: ~3500 lines of decompiled C

This is the main entry point / event loop of WA.exe. It handles:
- Command-line argument parsing
- Crash handling (`/handlecrash`, `/silentcrash`, `/disablecrashhandler`)
- Replay playback (`/play`, `/playat`, `/getlog`, `/getmap`, `/getscheme`, `/getvideo`, `/getlogs`)
- Sanitize/repair modes (`/sanitise`, `/repair`)
- Network hosting (`/host`, `wa:` protocol)
- WormKit module loading (`/wk`, `/nowk`, `/wkargs`)
- Frontend screen navigation loop (main menu, multiplayer, campaigns, etc.)
- Renderer selection (DirectDraw, OpenGL, D3D9, etc.)

## Crash Handler Mechanism

WA.exe's crash handler works by **relaunching itself** with `/handlecrash`:

1. Original WA.exe crashes (C++ exception via `__CxxThrowException`)
2. The crash handler (likely SEH-based) writes `ERRORLOG.TXT` and relaunches WA.exe with `/handlecrash`
3. The new WA.exe instance processes `/handlecrash` in this function
4. It checks `Options__GetCrashReportURL()` — if crash reporting is available, shows a dialog offering to send the report
5. It checks `DAT_0088e410` (WormKit modules loaded flag) — if set, shows the "Fatal error" dialog with Yes/No asking to disable WormKit modules

### Key flags:
- `/silentcrash` → sets `DAT_007a0f10+1 = 1` — suppresses the crash dialog
- `/disablecrashhandler` → sets `DAT_007a0f10+0 = 1` — disables the crash handler entirely
- `/crash` → intentionally crashes via `__CxxThrowException("Goodbye, cruel world!")`

### Crash dialog code (at ~0x4E6730):
```c
// Check for crash report URL
iVar11 = Options__GetCrashReportURL();
if (iVar11 != 0) {
    // Show "send bug report?" dialog (MB_YESNO | MB_ICONQUESTION = 0x14)
    iVar10 = MessageBoxA(NULL,
        "Worms Armageddon has encountered an error...\nWould you like to send the bug report?",
        "Fatal error", 0x14);
    if (iVar10 == IDYES) FUN_005a6470(); // send crash report
}

// Check if WormKit modules were loaded
if (DAT_0088e410 == 0) {
    // No WormKit — just show error (MB_ICONERROR = 0x10)
    MessageBoxA(NULL, error_msg, "Fatal error", 0x10);
} else {
    // WormKit loaded — ask to disable (MB_YESNO | MB_ICONQUESTION = 0x14)
    FUN_00444d30(); // builds error message mentioning WormKit
    iVar11 = MessageBoxA(NULL, error_msg, "Fatal error", 0x14);
    if (iVar11 == IDYES) {
        DAT_0088e410 = 0;
        FUN_004c8c70(); // disable WormKit in registry
        MessageBoxA(NULL, "WormKit modules have been disabled.", "Worms Armageddon", 0x40);
    }
}
```

## ERRORLOG.TXT

String at `0x00677FE0`: `"ERRORLOG.TXT"`
String at `0x00678484`: `"Can't open ERRORLOG.TXT"`
Written by the crash handler before relaunching (separate from this function).

## `/getlog` Mode

The `/getlog` flag sets:
- `DAT_0088cd4c = 1` (headless replay mode)
- `local_31 = 1` (getlog flag)
- Copies replay path to `DAT_0088af58`

Later in the function, `/getlog` mode opens the replay for writing the game log output.

## WormKit Module Loading

Around line 1200 of decompilation:
- Searches for `wk*.dll` files in the game directory
- Loads each via `LoadLibraryA`
- If loading fails, shows a MessageBox with the error
- Flag at `DAT_0088e410` tracks whether WormKit modules are loaded

## Key Addresses

| Address | Purpose |
|---------|---------|
| `0x007A0F10+0` | `/disablecrashhandler` flag |
| `0x007A0F10+1` | `/silentcrash` flag |
| `0x0088E410` | WormKit modules loaded flag |
| `0x0088AF58` | Replay path buffer |
| `0x0088CD4C` | Headless replay mode flag |
| `0x0088E17D` | Current directory buffer |
| `0x006B1FEC` | argc |
| `0x006B1FF0` | argv |
