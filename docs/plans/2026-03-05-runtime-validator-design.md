# Runtime Validator DLL Design

## Goal

Validate that our reverse-engineered struct layouts, field offsets, and address
constants in `openwa-types` are correct by checking them against the live
Worms Armageddon 3.8.1 process at runtime.

## Architecture

A 32-bit Rust cdylib (`wkOpenWAValidator.dll`) loaded by WormKit's HookLib into
the WA.exe process. On load it:

1. Validates address constants (vtables contain plausible function pointers,
   function addresses start with valid x86 prologues)
2. Hooks key constructors to capture object pointers
3. When constructors fire, validates struct field offsets against live memory
4. Writes results to `OpenWA_validation.log`

## Crate structure

```
crates/openwa-validator/
├── Cargo.toml      # cdylib, i686-pc-windows-msvc
└── src/
    └── lib.rs      # DLL entry, hooks, validation
```

## Dependencies

- `retour` (with `static-detour` feature) — inline function hooking
- `openwa-types` (path dependency) — the types being validated
- `windows` or raw FFI — for DllMain, file I/O

## Build

```sh
cargo build --target i686-pc-windows-msvc -p openwa-validator --release
```

Output: `target/i686-pc-windows-msvc/release/openwa_validator.dll`
Copy to game dir as `wkOpenWAValidator.dll`.

## Validation checks

### Address validation (immediate, on DLL load)

For each vtable address in `va::*_VTABLE`:
- Read the first entry (should be a function pointer in .text range)
- PASS if value is in 0x401000..0x619FFF

For each function address in `va::*`:
- Read first 1-3 bytes at that address
- PASS if starts with common x86 prologue (55 8B EC = push ebp; mov ebp, esp)
  or other valid patterns (83 EC, 56, 53, etc.)

### Struct validation (on constructor hook)

**CTask constructor (0x5625A0):**
- After constructor returns, read object
- Check vtable == 0x669F8C
- Check _unknown_08 == 0x10 (set by constructor)

**CGameTask constructor (0x4FED50):**
- Check vtable2 at offset 0xE8 == 0x669CF8
- Check total struct offsets are consistent

**DDGameWrapper constructor (0x56DEF0):**
- Check vtable at +0x00 == 0x66A30C
- Check ddgame pointer at +0x488 is non-null (after init)

**DDGame constructor (0x56E220):**
- Check landscape at +0x20 matches wrapper's landscape at +0x4CC
- Check known init values (field_7EFC == 1, etc.)

## Output format

```
=== OpenWA Validator ===
[PASS] va::CTASK_VTABLE (0x669F8C) -> first entry 0x562710 (in .text)
[PASS] va::CONSTRUCT_DD_GAME (0x56E220) -> prologue 55 8B EC
[FAIL] DDGame+0x020 (landscape): expected non-null, got 0x00000000
[INFO] 45/47 checks passed, 2 failed
```

## Deployment

1. Build the DLL
2. Copy to WA game directory as `wkOpenWAValidator.dll`
3. Launch WA via WormKit (or directly — HookLib loads wk*.dll)
4. Play through to trigger constructors (start a game)
5. Check `OpenWA_validation.log` in game directory

## Non-goals

- No signature scanning (hardcoded addresses for 3.8.1 only)
- No game modification (read-only checks)
- No complex hook chains
