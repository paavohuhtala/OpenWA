# ReplayLoader Port Design

## Summary

Port `ReplayLoader` (0x462DF0) and `ParseReplayPosition` (0x4E3490) from WA.exe
to Rust as hook-and-replace functions. ReplayLoader is a ~1800-line function that
parses `.WAgame` replay files, validates their format, extracts team/scheme/map
data, and produces `/getlog` formatted output. ParseReplayPosition is a small
utility that converts `MM:SS.FF` time strings to frame counts at 50fps.

## Target functions

### ReplayLoader (0x462DF0)

- **Signature:** `undefined4 ReplayLoader(uint param_1, int param_2)`
- **Convention:** stdcall (`RET 0x8`, callee cleans 2 params). Also has a plain
  `RET` at 0x465BC4 which is the SEH exception cleanup path.
- **param_1:** Global game state struct at `0x87D3F8` (NOT DDGame). Always the
  same global address — callers pass the literal constant `0x87D3F8`. The struct
  holds replay/team/scheme state; its relationship to DDGame is that DDGame reads
  from some of the same globals after ReplayLoader populates them.
- **param_2:** Mode — 1=play, 2=getmap, 3=getscheme, 4=repair
- **Returns:** 0 on success, negative error codes on failure

Error codes:
| Code | Meaning |
|------|---------|
| 0 | Success |
| -1 (0xFFFFFFFF) | File not found / read error |
| -2 (0xFFFFFFFE) | Invalid format / validation failure |
| -3 (0xFFFFFFFD) | Version too new / unsupported scheme version |
| -4 (0xFFFFFFFC) | Malloc failure |
| -5 (0xFFFFFFFB) | Map load failure |
| -6 (0xFFFFFFFA) | DAT_0088c790 > 0x33 |
| -7 (0xFFFFFFF9) | Repair with teams present |
| -9 (0xFFFFFFF7) | No scheme in file / game type mismatch |
| -8 (0xFFFFFFF8) | Not used by this function (gap in error codes) |
| -10 (0xFFFFFFF6) | Scheme save failure |

### ParseReplayPosition (0x4E3490)

- **Signature:** `int ParseReplayPosition(char *param_1)`
- **Convention:** stdcall (`RET 0x4`, callee cleans 1 param)
- **Returns:** Frame count (50fps), or -1 on parse error
- **Format:** `[H:]MM:SS[.FF]` — hours optional, fractional seconds optional

## Function structure

ReplayLoader breaks into 5 logical sections:

### Section 1: Header & file I/O (~100 lines)

1. Guard: `DAT_0088c790 > 0x33` → return -6
2. Set `param_1+0xDB48 = 1` (replay active flag)
3. Set `param_1+0xEF60 = 0` (init counter)
4. `SetCurrentDirectoryA` to game data dir
5. `fopen(param_1+0xDB60, "rb")` — replay file path
6. Read 4-byte header: lower 16 bits must be `0x4157` ("WA")
7. Version = upper 16 bits; must be ≤ 0x13 (19)
8. Store version at `param_1+0xDB50`, `param_1+0xDB54`; set `param_1+0xDB58 = 0xFFFFFFFF`
9. Read 4-byte payload size; validate against file size
10. For modes 3/4: seek past payload. For modes 1/2: malloc + fread payload.

### Section 2: Version 1 legacy path (~200 lines)

When `version == 1`:
- Read 4-byte team count from file, then 0x5728 bytes of team header into `0x8779E4`
- Validate team count (1-6) and team type bounds
- Iterate teams via `FUN_00461690` (team data reader)
- Copy scheme defaults from `SCHEME_V3_DEFAULTS`
- Read 0xD5D0-byte game state block via malloc, validate fields
- Copy ~20 field groups from game state block into `param_1` at various offsets
  (0x40, 0x44, 0x48C, 0x490, 0xD0FC, 0x4AF8, 0x4AFC, 0x4AFA, 0x8EFC, etc.)
- Free temp buffer

### Section 3: Version 2+ parsing (~500 lines)

When `version >= 2`:
1. Read second payload (team data) with size + content
2. Parse sub-version flags and format indicators
3. Read game version ID → `DAT_0088ABB0`; validate < 0x1F8
4. Read scheme presence flag → `DAT_0088AE0C`
5. If scheme present: read `SCHM` magic + version byte (1/2/3)
   - Version 1/2: call `FUN_004613D0` (scheme reader), maybe set defaults
   - Version 3: validate extended options via `Scheme__ValidateExtendedOptions`
6. Read map seed byte, random state
7. Read observer player names (up to 0xD teams, stride 0x78)
8. Team iteration (up to 6 teams, stride 0xD7B at `0x878120`):
   - Team flag byte, team type, alliance
   - 5 sub-field reads via `FUN_00461540`
   - Worm alliance byte
   - Per-worm names: 8 worms × 0x11 bytes (or auto-generated for old versions)
   - Worm count (validate 1-8), color, flag, grave, soundbank bytes
   - Team name (0x40), team config name (0x40)
   - Weapon data: 0x400 + 0x154 + 0x400 + 0x300 bytes
9. Validate team count matches expected
10. XOR integrity: `DAT_0088AF50 = payload_dword ^ 0xEF5B5C49`

### Section 4: Post-parse & map load (~200 lines)

1. If mode 1 (play): `Scheme__CheckWeaponLimits` validation
2. If mode 3 (getscheme): version compatibility fixups, `Scheme__SaveFile`
3. If mode 4 (repair): artclass name writing to replay file
4. Map loading: read `playback.thm`, call map loader (`FUN_00490890`)
5. Artclass index → terrain descriptor lookup via table at `0x6ACD38`
6. Close file

### Section 5: Log output (~600 lines)

Generates `/getlog` formatted output to `DAT_0088C370` (log file handle):
1. Date/time header via `_gmtime64`
2. Version string lookup (negative IDs → hardcoded strings, positive → table at
   `0x6AB480` or `0x6AB7A4` for BoomRacing mode)
3. Format line: `"Game: <version> + <mods>..."` and `"Replay: <format>..."`
4. Current WA version line: `"3.8.1"`
5. Artclass line (if applicable)
6. Per-team listing with alignment padding:
   - Team flag letter (uppercased via codepage)
   - Team name with quotes and padding
   - Player name assignment
   - CPU teams: difficulty level float
   - Human teams: host/first markers
7. Observer teams (negative team type)
8. Final newline + flush

**Critical:** This output must match byte-for-byte for headless replay testing.

## Helper functions

Several internal helpers are called repeatedly. These need bridges or ports:

| Address | Name | Purpose | Approach | Convention |
|---------|------|---------|----------|------------|
| `0x461340` | ReadBytes | Read N bytes from stream cursor into buffer | Port | Verify during impl |
| `0x4614D0` | ReadBytesValidated | Read + validate bounds | Port | Verify during impl |
| `0x4615B0` | ReadU32Validated | Read u32 from stream | Port | Verify during impl |
| `0x461540` | ReadByteRange | Read byte with signed range validation | Port | Verify during impl |
| `0x461620` | ReadTeamWormAlliance | Read worm alliance data | Port | Verify during impl |
| `0x461690` | ReadTeamData_V1 | Read team data (version 1 format) | Port | Verify during impl |
| `0x466460` | ProcessTeamColors | Post-process team color assignments | Bridge | Verify (check RET) |
| `0x4670F0` | ProcessSchemeDefaults | Apply scheme default values | Bridge | Verify (check RET) |
| `0x467280` | ProcessReplayFlags | Process replay feature flags | Bridge | Verify (check RET) |
| `0x467BC0` | RegisterObserverTeam | Register observer team entry | Bridge | Verify (check RET) |
| `0x468890` | ProcessAllianceData | Process alliance/team setup | Bridge | Verify (check RET) |
| `0x465E10` | ValidateTeamSetup | Validate team configuration | Bridge | Verify (check RET) |
| `0x593180` | LoadStringResource | Load MFC string resource by ID | Bridge | Verify (check RET) |
| `0x5978A0` | FormatString | sprintf-like string formatter | Bridge | Verify (check RET) |
| `0x5978F0` | FormatString2 | Another format variant | Bridge | Verify (check RET) |

**Note:** All helper function calling conventions must be verified via `RET imm16`
disassembly before implementation. Wrong conventions cause stack corruption.

**Policy:** Small functions (especially usercall) should be ported to Rust rather
than bridged. Bridging usercall functions requires naked asm trampolines which add
complexity and risk. Only bridge functions that are large or have many dependencies.

## Global buffers written

| Address | Size | Purpose |
|---------|------|---------|
| `0x87D3F8` | param_1 base | Game state struct (team/scheme/weapon data at various offsets) |
| `0x8779E4` | 0x5728 | Team header data |
| `0x87D438` | 0xD9DC | Secondary team/game data |
| `0x88AF50` | 4 | XOR'd game ID |
| `0x88AF54` | 4 | Replay sub-format flag |
| `0x88ABB0` | 4 | Game version ID |
| `0x88AE0C` | 4 | Scheme present flag |
| `0x88DC04` | 0x6E | Scheme defaults buffer |
| `0x88C790` | 4 | Version/ArtClass counter |
| `0x88D0B4` | 4 | Random seed |
| `0x88ABAC` | 4 | Saved random seed |

## Implementation plan

### Phase 0: Passthrough hook + ParseReplayPosition

**Goal:** Validate calling convention, port the trivial function.

1. Add `va::REPLAY_LOADER` and `va::PARSE_REPLAY_POSITION` to `address.rs`
2. Add all replay-related global addresses to `address.rs`
3. Create `replacements/replay.rs` with passthrough hook for ReplayLoader
4. Port `ParseReplayPosition` as full Rust replacement
5. Register in `replacements/mod.rs`
6. Test: headful + headless replay

### Phase 1: Header parsing + early returns

**Goal:** Replace the header validation and mode routing in Rust.

1. Port Section 1 (header & file I/O) to Rust
2. For modes 2 (getmap) and 4 (repair): delegate to original via trampoline
3. For mode 3 (getscheme): delegate to original initially
4. For mode 1 (play): continue to Rust Section 2/3
5. Test: headful replay with mode 1

### Phase 2: Payload parsing (version 2+)

**Goal:** Replace the core team/scheme data parsing.

Focus on version 2+ path (version 1 is legacy, unlikely to be encountered):
1. Port stream cursor helpers (ReadBytes, ReadU32, etc.) as Rust utilities
2. Port the team iteration loop (6 teams × 0xD7B stride)
3. Port scheme data reading (SCHM magic validation + delegation to existing bridges)
4. Port weapon data block copies
5. Bridge remaining sub-functions (ProcessTeamColors, ProcessSchemeDefaults, etc.)
6. Test: headless determinism — game must play identically

### Phase 3: Log output

**Goal:** Reproduce /getlog formatted output byte-for-byte.

This is the highest-risk phase. The output includes:
- Codepage-aware character conversion
- MFC string resources (localized strings)
- Precise column alignment with padding
- Version string table lookups
- Date formatting via `_gmtime64`

Approach:
1. Start with a passthrough to the original log section
2. Port incrementally, comparing output byte-for-byte via headless tests
3. Use WA's own string resource functions via FFI bridges
4. Test: headless diff must show zero differences

## Verification

| Phase | Test | Pass criteria |
|-------|------|---------------|
| 0 | Headful + headless replay | All existing validations pass, headless log matches |
| 1 | Headful replay | Game loads and plays replay correctly |
| 2 | Headless replay | Output log byte-identical to expected |
| 3 | Headless replay | Output log byte-identical to expected |

## Risks

1. **Calling convention**: Confirmed stdcall (`RET 0x8`). The plain `RET` at
   0x465BC4 is the SEH exception cleanup path, not the normal return.

2. **C++ exception handling**: The version 2+ path uses `__CxxThrowException_8`
   with SEH (`FS:[0]` exception chain) for validation errors. The version 1 path
   uses direct returns. The SEH handler performs cleanup (freeing malloc'd buffers,
   closing file handles) before returning the error code. The Rust replacement must
   replicate this cleanup on every error path. Use a cleanup guard pattern (e.g., a
   struct with `Drop` impl, or explicit cleanup before each early return) to ensure
   no resource leaks.

3. **Log output fidelity**: The /getlog output must match byte-for-byte. Codepage
   conversion, string resources, and column alignment must be exact. This is the
   hardest part and may require falling back to WA's own formatting functions.

4. **Version 1 format**: Rarely encountered (pre-3.5 replays). Could be left as
   a bridge to original initially, ported later if needed.

5. **Helper function dependencies**: ~15 helper functions called. Trivial stream
   helpers should be ported; complex ones (string resources, scheme processing)
   should be bridged via FFI. All conventions must be verified before use.

6. **Stdcall trampoline delegation**: When delegating to the original function for
   unimplemented modes, the hook is `extern "stdcall"` and the MinHook trampoline
   preserves the original stdcall convention. The Rust hook should call the
   trampoline as a regular function pointer (the trampoline handles its own stack
   cleanup internally — MinHook trampolines are designed for this). This is the
   same pattern used by existing stdcall hooks in `game_state_hooks.rs`.
