# Port Configuration System to Rust

## Context

The WA configuration system has three subsystems: theme file I/O, Windows Registry management, and game options loading. All are self-contained leaf functions with no graphics or game state dependencies beyond simple struct field writes.

## Scope — 7 Functions

### Theme File I/O (3 functions)

| Function | Address | Convention | Params | What it does |
|----------|---------|------------|--------|-------------|
| `Theme__GetFileSize` | 0x44BA80 | cdecl | 0 | Opens `data\current.thm`, returns file length (0 if missing) |
| `Theme__Load` | 0x44BB20 | stdcall | 1 (dest buffer) | Reads entire theme file into buffer; MessageBox on failure |
| `Theme__Save` | 0x44BBC0 | stdcall | 2 (buffer, size) | Writes buffer to theme file; MessageBox on create failure |

Replace MFC CFile with `std::fs`. Replace `AfxMessageBox` with `MessageBoxA` from the `windows` crate.

### Registry Management (2 functions)

| Function | Address | Convention | Params | What it does |
|----------|---------|------------|--------|-------------|
| `Registry__DeleteKeyRecursive` | 0x4E4D10 | stdcall | 2 (hkey, subkey) | Recursively enumerates and deletes registry subkeys |
| `Registry__CleanAll` | 0x4C90D0 | stdcall | 1 (struct ptr) | Deletes Data/Options/ExportVideo/VSyncAssist subsections + clears INI |

Pure Win32 registry API. Original uses malloc/realloc for key name buffer — Rust `Vec<u8>` replaces this.

### Game Options (2 functions)

| Function | Address | Convention | Params | What it does |
|----------|---------|------------|--------|-------------|
| `GameInfo__LoadOptions` | 0x460AC0 | stdcall | 1 (GameInfo ptr) | Reads ~12 registry values into GameInfo struct fields |
| `Options__GetCrashReportURL` | 0x5A63F0 | cdecl | 0 | Reads CrashReportURL from Options registry key |

LoadOptions reads from `HKCU\Software\Team17SoftwareLTD\WormsArmageddon\Options` via MFC `GetProfileIntW`. We replace with direct `RegOpenKeyExA` + `RegQueryValueExA`.

Key value transformations in LoadOptions:
- `BackgroundDebrisParallax`: clamp to i16 range, then `<< 16` (fixed-point 16.16)
- `CameraUnlockMouseSpeed`: clamp to max 0xB504, then square
- `InfoSpy`: stored as `!= 0` (bool)
- All others: simple cast to byte or store as u32

LoadOptions also copies various globals into the GameInfo struct and formats the speech path via sprintf.

## Dependencies

### `windows` crate

Add to `openwa-lib` with features:
- `Win32_System_Registry` — RegOpenKeyExA, RegQueryValueExA, RegEnumKeyExA, RegDeleteKeyA, RegCloseKey
- `Win32_UI_WindowsAndMessaging` — MessageBoxA, WriteProfileSectionA

Existing raw FFI in `resource.rs` and `rebase.rs` stays as-is for now — migration to `windows` crate is a separate future task.

### Registry helper in openwa-lib

`read_profile_int(section: &str, key: &str, default: u32) -> u32`

Opens `HKCU\Software\Team17SoftwareLTD\WormsArmageddon\{section}`, reads a DWORD via `RegQueryValueExA`, returns default on any failure. Matches MFC `GetProfileIntW` behavior for the registry-mode path.

## Hook Strategy

All 7 functions are full Rust replacements — no trampolines needed. All conventions are plain cdecl or stdcall (confirmed via RET instruction analysis).

## File Layout

| File | Change |
|------|--------|
| `crates/openwa-types/src/address.rs` | Add 7 address constants |
| `crates/openwa-lib/Cargo.toml` | Add `windows` crate dependency |
| `crates/openwa-lib/src/wa/registry.rs` (new) | `read_profile_int`, `delete_key_recursive` |
| `crates/openwa-lib/src/wa/mod.rs` | Add `pub mod registry;` |
| `crates/openwa-dll/src/replacements/config.rs` (new) | 7 hook functions + `install()` |
| `crates/openwa-dll/src/replacements/mod.rs` | Add `pub mod config;` |

## Global Data References in LoadOptions

| Global Address | Description | GameInfo Offset |
|---------------|-------------|-----------------|
| 0x0088E282 | Base directory string | +0xF404 (speech path) |
| 0x0088DFF3 | 64 bytes copied verbatim | +0xF485 |
| 0x0064DA58 | `"data\land.dat"` string (14 bytes) | +0xDAEC |
| 0x007C0D38 | Unknown byte | +0xF3A0 |
| 0x0088E39C..0x0088E3AC | 5 DWORDs | +0xF3B4..+0xF3D0 |
| 0x0088C374 | Guard flag for conditional copies | — |
| 0x0088E3B8..0x0088E3C4 | 4 DWORDs (conditional) | +0xF3F4..+0xF400 |
| 0x0088E390 | DWORD | +0xDAE8 |
| 0x0088E3B0..0x0088E3B4 | 2 DWORDs | +0xF3D4..+0xF3D8 |
| 0x0088E400..0x0088E408 | 3 DWORDs | +0xF3C4..+0xF3CC |
| 0x0088E44C | DWORD | +0xF3E4 |

These are read via rebased pointers (`rb(addr)`) and written to the GameInfo struct at the given offsets.

## Verification

1. Build: `cargo build --release -p openwa-dll` — clean
2. Deploy wkOpenWA.dll to game directory
3. Launch game, check OpenWA.log for config hook messages
4. Verify game options persist (change Detail Level, restart, confirm it's saved)
5. Verify theme loads correctly (visual appearance unchanged)
6. Test Registry__CleanAll if reachable (it's called from a "reset all settings" path)
7. Check CrashReportURL is read correctly (visible in crash dialog if triggered)

## Decisions

- **`windows` crate**: Add now, migrate existing FFI later
- **Theme error messages**: Use `MessageBoxA` from `windows` crate (replaces AfxMessageBox)
- **Registry reads**: Direct Win32 API, not MFC GetProfileIntW (Steam always uses registry mode)
- **GameInfo struct**: Write to raw pointer at known offsets (don't type the full 0xF500-byte struct)
- **std::fs over CFile**: For theme I/O
