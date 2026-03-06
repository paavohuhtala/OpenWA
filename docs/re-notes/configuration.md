# WA Configuration System

WA uses three configuration layers: Windows Registry (primary), theme files, and weapon scheme files.

## Registry

### Path

```
HKEY_CURRENT_USER\Software\Team17SoftwareLTD\WormsArmageddon\
├── Data
├── Options
├── ExportVideo
└── VSyncAssist
```

Company name string at `0x6623DC` ("Team17SoftwareLTD"), app name at `0x66237C`.

### MFC CWinApp Integration

WA uses MFC's `CWinApp` registry helpers. Dual-mode: if `CWinApp+0x54 == 0`, falls back to INI file via `GetPrivateProfileIntA`. Otherwise uses the registry.

| Function | Address | Xrefs | Notes |
|----------|---------|-------|-------|
| `CWinApp::GetAppRegistryKey` | 0x5CC4FB | — | Opens `HKCU\Software\{company}\{app}` |
| `CWinApp::GetSectionKey` | 0x5CC58C | — | Opens subsection (Options, Data, etc.) |
| `CWinApp::GetProfileIntW` | 0x5CC5D2 | 68 | Reads int from registry or INI |
| `CWinApp::WriteProfileInt` | 0x5CC63B | 55 | Writes int to registry or INI |
| `CWinApp::WriteProfileStringA` | 0x5CC6C2 | — | Writes string to registry or INI |
| `CWinApp::GetProfileStringA` | 0x5CC758 | — | Reads string from registry or INI |

Registry access flags: `0x2001F` (KEY_READ | KEY_WRITE).

### Registry Cleanup

`Registry__CleanAll` (0x4C90D0) deletes all 4 subsections recursively via `Registry__DeleteKeyRecursive` (0x4E4D10), also clears the "NetSettings" INI section.

## Game Options

`GameInfo__LoadOptions` (0x460AC0) reads options from the "Options" registry section into the GameInfo struct. Called from `GameInfo__InitSession` (0x4608E0).

| Registry Key | Default | GameInfo Offset | Type | Notes |
|-------------|---------|-----------------|------|-------|
| DetailLevel | 5 | +0xF3A1 | byte | |
| EnergyBar | 1 | +0xF3A2 | byte | |
| InfoTransparency | 0 | +0xF3A3 | byte | |
| InfoSpy | 1 | +0xF3A4 | bool | Stored as `!= 0` |
| ChatPinned | 0 | +0xF3A5 | byte | |
| ChatLines | 0 | +0xF3A8 | u32 | |
| PinnedChatLines | -1 | +0xF3AC | u32 | |
| HomeLock | 0 | +0xF3B0 | byte | |
| BackgroundDebrisParallax | 0x50 | +0xF3E8 | fixed16.16 | Clamped to i16 range, then `<< 16` |
| TopmostExplosionOnomatopoeia | 0 | +0xF3EC | u32 | |
| CaptureTransparentPNGs | 0 | +0xF3DC | u32 | |
| CameraUnlockMouseSpeed | 0x10 | +0xF3E0 | u32 | Squared; clamped to max 0xB504 before squaring |

Additional values loaded from globals (not registry) into offsets: +0xF3B4..+0xF3D0, +0xF3D4..+0xF3D8, +0xF3F4..+0xF400 (conditional on `DAT_0088C374`), +0xDAE8, +0xF3E4.

The struct also stores:
- Speech path at +0xF404: `sprintf("%s\\user\\speech", baseDir)`
- Land data path at +0xDAEC: `"data\land.dat"` (from string at 0x64DA58)

`Options__GetCrashReportURL` (0x5A63F0) reads "CrashReportURL" from the Options section for crash reporting.

## Theme File (`data\current.thm`)

Binary format, variable length. Contains visual/audio theme settings.

| Function | Address | Notes |
|----------|---------|-------|
| `Theme__GetFileSize` | 0x44BA80 | Opens file, returns length (0 if missing) |
| `Theme__Load` | 0x44BB20 | Reads entire file into buffer; `AfxMessageBox` if missing |
| `Theme__Save` | 0x44BBC0 | Writes buffer to file; `AfxMessageBox` on create failure |

### Save Flow

`Frontend__OnOptionsAccept` (0x48DAB0) triggers on screen 0x27:
1. Calls `FUN_0048D7C0` (unknown setup)
2. Calls `Frontend__ApplyOptions` (0x440940) with frontend data
3. Calls `Frontend__SaveTheme` (0x490E60) with path `"data\current.thm"`
4. Calls `FrontendChangeScreen` to navigate away

## Weapon Schemes

Stored as `.wsc` files under `User\Schemes\`.

| Function | Address | Path Pattern |
|----------|---------|-------------|
| `Scheme__Load` | 0x4D4CD0 | `User\Schemes\%s.wsc` |
| `Scheme__LoadNumbered` | 0x4D4E00 | `User\Schemes\{%02d} %s.wsc` |

## Data Files

| File | String Address | Notes |
|------|---------------|-------|
| `data\land.dat` | 0x64DA58 | Landscape/terrain data |
| `custom.dat` | 0x64DA88 | Custom content |
| `steam.dat` | 0x662808 | Steam integration data |
