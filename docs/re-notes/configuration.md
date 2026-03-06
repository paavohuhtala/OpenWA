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

## Weapon Schemes (.wsc)

Stored as `.wsc` files under `User\Schemes\`. Full format spec: [worms2d.info/Game_scheme_file](https://worms2d.info/Game_scheme_file).

### Functions

| Function | Address | Signature | Notes |
|----------|---------|-----------|-------|
| `Scheme__ReadFile` | 0x4D3890 | stdcall(dest, path, flag, out_ptr) → u32, RET 0x10 | Reads .wsc into dest+0x14 |
| `Scheme__SaveFile` | 0x4D44F0 | thiscall(this, name, flag), RET 0x8 | Writes struct to .wsc |
| `Scheme__DetectVersion` | 0x4D4480 | Uses ESI (non-standard) → 1/2/3 | Compares V3 extension bytes |
| `Scheme__FileExists` | 0x4D4CD0 | stdcall(name) → 0/1, RET 0x4 | Path: `User\Schemes\%s.wsc` |
| `Scheme__FileExistsNumbered` | 0x4D4E00 | stdcall(...) | Path: `User\Schemes\{%02d} %s.wsc` |
| `Scheme__InitFromData` | 0x4D5020 | fastcall(?, data, dest, name) | Copies payload + V3 defaults |
| `Scheme__CheckWeaponLimits` | 0x4D50E0 | — → 0/1 | Validates ammo vs max table (39 weapons) |
| `Scheme__ValidateExtendedOptions` | 0x4D5110 | Uses EAX → 0/1 | V3 field range checks |
| `Scheme__ScanDirectory` | 0x4D54E0 | — | Finds `{NN} name.wsc` files |
| `Scheme__ExtractBuiltins` | 0x4D5720 | — | Extracts PE resources (IDs 0x3CA-0x3D6) |

### Data

| Symbol | Address | Size | Notes |
|--------|---------|------|-------|
| `SCHEME_V3_DEFAULTS` | 0x649AB8 | 110 bytes | Default extended options for V1/V2 |
| `SCHEME_WEAPON_AMMO_LIMITS` | 0x6AD130 | 39 bytes | Max ammo per weapon (V1 set) |

### File Format

Binary, little-endian. Header: `"SCHM"` (4 bytes) + version byte (1 byte) + payload.

| Version | Payload | Total | Content |
|---------|---------|-------|---------|
| V1 (0x01) | 0xD8 (216) | 221 | 36 options + 45 weapons × 4 |
| V2 (0x02) | 0x124 (292) | 297 | V1 + 19 super weapons × 4 |
| V3 (0x03) | 0x192 (402) | 407 | V2 + 110 extended options |

### Payload Layout

**Game Options (36 bytes, payload +0x00):**

| Offset | Size | Field | Notes |
|--------|------|-------|-------|
| +0x00 | 1 | hot_seat_delay | Seconds between turns |
| +0x01 | 1 | retreat_time | Seconds after weapon use |
| +0x02 | 1 | rope_retreat_time | Seconds after rope weapon use |
| +0x03 | 1 | display_total_round_time | bool |
| +0x04 | 1 | automatic_replays | bool |
| +0x05 | 1 | fall_damage | Damage at critical velocity |
| +0x06 | 1 | artillery_mode | bool |
| +0x07 | 1 | bounty_mode | 0x00/0x5F/0x89 |
| +0x08 | 1 | stockpiling | 0=Off, 1=On, 2=Anti |
| +0x09 | 1 | worm_select | 0=Sequential, 1=Manual, 2=Random |
| +0x0A | 1 | sudden_death_event | 0=RoundEnds, 1=Nuke, 2=HP→1, 3=Nothing |
| +0x0B | 1 | water_rise_rate | Flooding speed |
| +0x0C | 1 | weapon_crate_probability | i8, -100 to 100 |
| +0x0D | 1 | donor_cards | bool |
| +0x0E | 1 | health_crate_probability | i8 |
| +0x0F | 1 | health_crate_energy | Energy from health crate |
| +0x10 | 1 | utility_crate_probability | i8 |
| +0x11 | 1 | hazardous_object_types | Bitmask |
| +0x12 | 1 | mine_delay | i8, seconds (0x80+=random) |
| +0x13 | 1 | dud_mines | bool |
| +0x14 | 1 | manual_worm_placement | bool |
| +0x15 | 1 | worm_energy | 0=instant death |
| +0x16 | 1 | turn_time | Seconds |
| +0x17 | 1 | round_time | Minutes (0=immediate SD) |
| +0x18 | 1 | number_of_wins | Rounds to win match |
| +0x19 | 1 | blood | false=pink, true=red |
| +0x1A | 1 | aqua_sheep | bool |
| +0x1B | 1 | sheep_heaven | bool |
| +0x1C | 1 | god_worms | bool |
| +0x1D | 1 | indestructible_land | bool |
| +0x1E | 1 | upgraded_grenade | bool |
| +0x1F | 1 | upgraded_shotgun | bool |
| +0x20 | 1 | upgraded_clusters | bool |
| +0x21 | 1 | upgraded_longbow | bool |
| +0x22 | 1 | team_weapons | bool |
| +0x23 | 1 | super_weapons | bool |

**Per-Weapon (4 bytes each, payload +0x24):**

`[ammo, power, delay, crate_probability]` — 45 weapons (V1), 64 weapons (V2+).

**V3 Extended Options (110 bytes, payload +0x124):**

Physics (gravity, friction, wind), RubberWorm modifiers, glitch toggles, and gameplay extensions. See `scheme.rs::ExtendedOptions` for full field list. Uses fixed-point 16.16 and tri-state (0/1/0x80) types.

### Callers of Scheme__ReadFile

| Address | Context |
|---------|---------|
| 0x4A1D18 | Frontend/options dialog |
| 0x4B4B92 | Multiplayer scheme loading |
| 0x4CC425 | Mission/campaign loading |

## Data Files

| File | String Address | Notes |
|------|---------------|-------|
| `data\land.dat` | 0x64DA58 | Landscape/terrain data |
| `custom.dat` | 0x64DA88 | Custom content |
| `steam.dat` | 0x662808 | Steam integration data |
