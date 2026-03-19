# ReplayLoader Port Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development
> (if subagents available) or superpowers:executing-plans to implement this plan.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port ReplayLoader (0x462DF0) and ParseReplayPosition (0x4E3490) from
WA.exe to Rust, enabling .WAgame file parsing in Rust code.

**Architecture:** Hook-and-replace strategy. ReplayLoader is stdcall(2 params),
~1800 lines. We decompose it into: (1) a ReplayStream cursor abstraction porting
the 6 usercall stream helpers, (2) ParseReplayPosition as a standalone pure
function, (3) the main ReplayLoader function replacing incrementally — header
first, then payload parsing, delegating unimplemented modes to the original via
trampoline.

**Tech Stack:** Rust (i686-pc-windows-msvc), MinHook, openwa-core types, WA.exe
FFI bridges for complex sub-functions.

**Spec:** `docs/superpowers/specs/2026-03-19-replay-loader-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/openwa-core/src/engine/replay.rs` | Create | ReplayStream struct, ParseReplayPosition, replay data types |
| `crates/openwa-core/src/engine/mod.rs` | Modify | Add `pub mod replay` |
| `crates/openwa-core/src/address.rs` | Modify | Add replay global addresses |
| `crates/openwa-wormkit/src/replacements/replay.rs` | Create | Hook installation, ReplayLoader replacement |
| `crates/openwa-wormkit/src/replacements/mod.rs` | Modify | Add `mod replay` + `replay::install()?` |

---

## Task 1: Add Replay Addresses to address.rs

**Files:**
- Modify: `crates/openwa-core/src/address.rs`

- [ ] **Step 1: Add replay global addresses**

In the `va` module (top-level function addresses section), the addresses
`REPLAY_LOADER` (0x462DF0) and `PARSE_REPLAY_POSITION` (0x4E3490) already exist.
Add the remaining replay-related globals and helper function addresses:

```rust
// In the va module, near the existing REPLAY_LOADER entries:

/// Stream helper: read length-prefixed string. usercall(EDI=ctx) + stdcall(dest, max_len). RET 0x8.
pub const REPLAY_READ_BYTES: u32 = 0x0046_1340;
/// Stream helper: read byte with range validation. usercall(EAX=ctx) + stdcall(dest, min, max). RET 0xC.
pub const REPLAY_READ_BYTE_VALIDATED: u32 = 0x0046_14D0;
/// Stream helper: read byte with signed range validation. usercall(EAX=ctx) + stdcall(dest, min, max). RET 0xC.
pub const REPLAY_READ_BYTE_RANGE: u32 = 0x0046_1540;
/// Stream helper: read u16 with range validation. usercall(EAX=ctx) + stdcall(dest, min, max). RET 0xC.
pub const REPLAY_READ_U16_VALIDATED: u32 = 0x0046_15B0;
/// Stream helper: read worm name (0x11 bytes or length-prefixed). usercall(EAX=ctx) + thiscall(this, flag). RET 0x4.
pub const REPLAY_READ_WORM_NAME: u32 = 0x0046_1620;
/// Validate team type byte range. fastcall(ECX=type). Plain RET.
pub const REPLAY_VALIDATE_TEAM_TYPE: u32 = 0x0046_1690;
/// Post-process team color assignments. stdcall(1 param). RET 0x4.
pub const REPLAY_PROCESS_TEAM_COLORS: u32 = 0x0046_6460;
/// Apply scheme default values. No params (uses globals). Plain RET.
pub const REPLAY_PROCESS_SCHEME_DEFAULTS: u32 = 0x0046_70F0;
/// Process replay feature flags. No params (uses globals). Plain RET.
pub const REPLAY_PROCESS_FLAGS: u32 = 0x0046_7280;
/// Register observer team entry. stdcall(1 param). RET 0x4.
pub const REPLAY_REGISTER_OBSERVER: u32 = 0x0046_7BC0;
/// Process alliance/team setup. No params (uses globals). Plain RET.
pub const REPLAY_PROCESS_ALLIANCE: u32 = 0x0046_8890;
/// Validate team configuration. stdcall(1 param).
pub const REPLAY_VALIDATE_TEAM_SETUP: u32 = 0x0046_5E10;
```

Add replay-related global data addresses:

```rust
// In a globals section (or near existing DAT_ references):

/// Global game state struct passed to ReplayLoader. Always 0x87D3F8.
pub const G_REPLAY_STATE: u32 = 0x0087_D3F8;
/// Team header data buffer (0x5728 bytes), cleared by ReplayLoader.
pub const G_TEAM_HEADER_DATA: u32 = 0x0087_79E4;
/// Secondary team/game data buffer (0xD9DC bytes).
pub const G_TEAM_SECONDARY_DATA: u32 = 0x0087_D438;
/// XOR'd game ID (payload ^ 0xEF5B5C49).
pub const G_REPLAY_GAME_ID: u32 = 0x0088_AF50;
/// Replay sub-format flag.
pub const G_REPLAY_SUB_FORMAT: u32 = 0x0088_AF54;
/// Game version ID from replay.
pub const G_REPLAY_VERSION_ID: u32 = 0x0088_ABB0;
/// Scheme present flag (1 = has scheme data).
pub const G_REPLAY_SCHEME_PRESENT: u32 = 0x0088_AE0C;
/// Scheme defaults buffer (0x6E bytes).
pub const G_SCHEME_DEFAULTS_BUF: u32 = 0x0088_DC04;
/// Version/ArtClass counter. ReplayLoader fails if > 0x33.
pub const G_ARTCLASS_COUNTER: u32 = 0x0088_C790;
/// Random seed global.
pub const G_RANDOM_SEED: u32 = 0x0088_D0B4;
/// Saved random seed.
pub const G_SAVED_RANDOM_SEED: u32 = 0x0088_ABAC;
/// Replay filename buffer.
pub const G_REPLAY_FILENAME: u32 = 0x0088_AF58;
```

- [ ] **Step 2: Label all replay functions in Ghidra**

Use `batch_rename_function_components` to name all replay-related functions:
- `FUN_00461340` → `Replay__ReadPrefixedString`
- `FUN_004614D0` → `Replay__ReadByteValidated`
- `FUN_00461540` → `Replay__ReadByteRange`
- `FUN_004615B0` → `Replay__ReadU16Validated`
- `FUN_00461620` → `Replay__ReadWormName`
- `FUN_00461690` → `Replay__ValidateTeamType`
- `FUN_00466460` → `Replay__ProcessTeamColors`
- `FUN_004670F0` → `Replay__ProcessSchemeDefaults`
- `FUN_00467280` → `Replay__ProcessReplayFlags`
- `FUN_00467BC0` → `Replay__RegisterObserver`
- `FUN_00468890` → `Replay__ProcessAllianceData`
- `FUN_00465E10` → `Replay__ValidateTeamSetup`

- [ ] **Step 3: Commit**

```bash
git add crates/openwa-core/src/address.rs
git commit -m "feat: add replay system addresses for stream helpers and globals"
```

---

## Task 2: Create ReplayStream and Port Stream Helpers

**Files:**
- Create: `crates/openwa-core/src/engine/replay.rs`
- Modify: `crates/openwa-core/src/engine/mod.rs`

The 6 stream helper functions all operate on a shared 3-DWORD context:
`[data_ptr: *const u8, total_size: u32, cursor: u32]`. Port them all as methods
on a Rust `ReplayStream` struct.

- [ ] **Step 1: Define ReplayStream struct and error type**

```rust
// crates/openwa-core/src/engine/replay.rs

/// Error codes returned by ReplayLoader.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    FileNotFound = -1,       // 0xFFFFFFFF
    InvalidFormat = -2,      // 0xFFFFFFFE
    VersionTooNew = -3,      // 0xFFFFFFFD
    MallocFailure = -4,      // 0xFFFFFFFC
    MapLoadFailure = -5,     // 0xFFFFFFFB
    ArtClassLimit = -6,      // 0xFFFFFFFA
    RepairWithTeams = -7,    // 0xFFFFFFF9
    NoScheme = -9,           // 0xFFFFFFF7
    SchemeSaveFailure = -10, // 0xFFFFFFF6
}

/// Replay file magic: "WA" in little-endian.
pub const REPLAY_MAGIC: u16 = 0x4157;
/// Maximum supported replay version.
pub const REPLAY_MAX_VERSION: u16 = 0x13;
/// XOR key for game ID integrity check.
pub const REPLAY_XOR_KEY: u32 = 0xEF5B_5C49;

/// Cursor over a replay payload byte buffer.
///
/// Mirrors the 3-DWORD stream context used by WA's stream helper functions:
/// `[data_ptr, total_size, cursor_offset]`.
pub struct ReplayStream<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> ReplayStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.cursor)
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Advance cursor by `n` bytes, returning the slice. Fails if not enough data.
    fn advance(&mut self, n: usize) -> Result<&'a [u8], ReplayError> {
        let end = self.cursor + n;
        if end > self.data.len() {
            return Err(ReplayError::InvalidFormat);
        }
        let slice = &self.data[self.cursor..end];
        self.cursor = end;
        Ok(slice)
    }

    /// Read a single byte. Port of parts of FUN_004614D0 / FUN_00461540.
    pub fn read_u8(&mut self) -> Result<u8, ReplayError> {
        let slice = self.advance(1)?;
        Ok(slice[0])
    }

    /// Read a single byte, validating it is in [min, max]. Port of FUN_004614D0.
    pub fn read_u8_validated(&mut self, min: u8, max: u8) -> Result<u8, ReplayError> {
        let val = self.read_u8()?;
        if val < min || val > max {
            return Err(ReplayError::InvalidFormat);
        }
        Ok(val)
    }

    /// Read a little-endian u16. Part of FUN_004615B0.
    pub fn read_u16(&mut self) -> Result<u16, ReplayError> {
        let slice = self.advance(2)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    /// Read a little-endian u16, validating range. Port of FUN_004615B0.
    pub fn read_u16_validated(&mut self, min: u16, max: u16) -> Result<u16, ReplayError> {
        let val = self.read_u16()?;
        if val < min || val > max {
            return Err(ReplayError::InvalidFormat);
        }
        Ok(val)
    }

    /// Read a little-endian u32.
    pub fn read_u32(&mut self) -> Result<u32, ReplayError> {
        let slice = self.advance(4)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    /// Read a little-endian i32.
    pub fn read_i32(&mut self) -> Result<i32, ReplayError> {
        Ok(self.read_u32()? as i32)
    }

    /// Read `n` bytes into a destination buffer. Fails if not enough data.
    pub fn read_into(&mut self, dest: &mut [u8]) -> Result<(), ReplayError> {
        let slice = self.advance(dest.len())?;
        dest.copy_from_slice(slice);
        Ok(())
    }

    /// Read a length-prefixed string into buffer. Port of FUN_00461340.
    ///
    /// Reads 1 byte (length), then `length` bytes of string data, null-terminates.
    /// `max_len` is the destination buffer capacity (excluding null).
    pub fn read_prefixed_string(&mut self, dest: &mut [u8]) -> Result<usize, ReplayError> {
        let len = self.read_u8()? as usize;
        if len > dest.len().saturating_sub(1) {
            return Err(ReplayError::InvalidFormat);
        }
        let slice = self.advance(len)?;
        dest[..len].copy_from_slice(slice);
        dest[len] = 0; // null-terminate
        Ok(len)
    }

    /// Read a worm name: either 0x11 fixed bytes or length-prefixed. Port of FUN_00461620.
    pub fn read_worm_name(&mut self, dest: &mut [u8; 0x11], use_fixed: bool) -> Result<(), ReplayError> {
        if use_fixed {
            let slice = self.advance(0x11)?;
            dest.copy_from_slice(slice);
        } else {
            dest.fill(0);
            self.read_prefixed_string(dest)?;
        }
        Ok(())
    }

    /// Skip `n` bytes.
    pub fn skip(&mut self, n: usize) -> Result<(), ReplayError> {
        self.advance(n)?;
        Ok(())
    }
}

/// Validate team type byte. Port of FUN_00461690.
///
/// Returns true if the team type value is in a valid range:
/// - Non-negative: 0..12 (0 through 12 exclusive → type < 13)
/// - Negative: -100 or absolute value < 100
pub fn validate_team_type(team_type: i8) -> bool {
    if team_type >= 0 {
        team_type < 13
    } else {
        team_type == -100 || -(team_type as i32) <= 100
    }
}
```

- [ ] **Step 2: Add module declaration**

In `crates/openwa-core/src/engine/mod.rs`, add:
```rust
pub mod replay;
```

And in the re-exports section if desired (or leave access via `engine::replay::`).

- [ ] **Step 3: Write unit tests for ReplayStream**

Add tests at the bottom of `replay.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_u8() {
        let data = [0x42, 0xFF];
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u8().unwrap(), 0x42);
        assert_eq!(s.read_u8().unwrap(), 0xFF);
        assert!(s.read_u8().is_err());
    }

    #[test]
    fn test_read_u8_validated() {
        let data = [5, 0, 20];
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u8_validated(0, 10).unwrap(), 5);
        assert_eq!(s.read_u8_validated(0, 10).unwrap(), 0);
        assert!(s.read_u8_validated(0, 10).is_err()); // 20 > 10
    }

    #[test]
    fn test_read_u16_le() {
        let data = [0x57, 0x41]; // "WA" = 0x4157
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u16().unwrap(), 0x4157);
    }

    #[test]
    fn test_read_u32_le() {
        let data = [0x78, 0x56, 0x34, 0x12];
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u32().unwrap(), 0x12345678);
    }

    #[test]
    fn test_read_prefixed_string() {
        let data = [3, b'f', b'o', b'o', 0, b'x']; // len=3, "foo"
        let mut s = ReplayStream::new(&data);
        let mut buf = [0u8; 16];
        let len = s.read_prefixed_string(&mut buf).unwrap();
        assert_eq!(len, 3);
        assert_eq!(&buf[..4], b"foo\0");
        assert_eq!(s.cursor(), 4);
    }

    #[test]
    fn test_read_worm_name_fixed() {
        let mut data = [0u8; 0x11];
        data[0] = b'W';
        data[1] = b'o';
        data[2] = b'r';
        data[3] = b'm';
        let mut s = ReplayStream::new(&data);
        let mut name = [0u8; 0x11];
        s.read_worm_name(&mut name, true).unwrap();
        assert_eq!(name[0], b'W');
        assert_eq!(s.cursor(), 0x11);
    }

    #[test]
    fn test_validate_team_type() {
        assert!(validate_team_type(0));
        assert!(validate_team_type(12));
        assert!(!validate_team_type(13));
        assert!(validate_team_type(-1));
        assert!(validate_team_type(-99));
        assert!(validate_team_type(-100));
        assert!(!validate_team_type(-101));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p openwa-core -- replay
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/openwa-core/src/engine/replay.rs crates/openwa-core/src/engine/mod.rs
git commit -m "feat: add ReplayStream cursor and stream helper ports"
```

---

## Task 3: Port ParseReplayPosition

**Files:**
- Modify: `crates/openwa-core/src/engine/replay.rs`

Port the `ParseReplayPosition` function (0x4E3490) as pure Rust. This converts
time strings like `"1:30.25"` to frame counts at 50fps.

- [ ] **Step 1: Write failing tests**

Add to the test module in `replay.rs`:

```rust
#[test]
fn test_parse_replay_position_seconds() {
    assert_eq!(parse_replay_position(b"30\0"), 30 * 50);
}

#[test]
fn test_parse_replay_position_minutes_seconds() {
    assert_eq!(parse_replay_position(b"1:30\0"), (60 + 30) * 50);
}

#[test]
fn test_parse_replay_position_with_frames() {
    assert_eq!(parse_replay_position(b"1:30.5\0"), (60 + 30) * 50 + 25);
}

#[test]
fn test_parse_replay_position_zero() {
    assert_eq!(parse_replay_position(b"0\0"), 0);
}

#[test]
fn test_parse_replay_position_invalid() {
    assert_eq!(parse_replay_position(b"abc\0"), -1);
}

#[test]
fn test_parse_replay_position_seconds_over_59() {
    // After a colon, seconds > 59 is invalid
    assert_eq!(parse_replay_position(b"1:60\0"), -1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p openwa-core -- parse_replay
```

Expected: FAIL (function not defined).

- [ ] **Step 3: Implement parse_replay_position**

Add to `replay.rs`:

```rust
/// Parse a replay position time string to frame count (50fps).
///
/// Format: `[MM:]SS[.FF]` where:
/// - MM = minutes (multiplied by 60)
/// - SS = seconds (max 59 after a colon)
/// - FF = fractional (multiplied by 50 for frames, divided by power of 10)
/// - Maximum 2 colons allowed
///
/// Returns frame count, or -1 on parse error.
/// Port of WA.exe ParseReplayPosition (0x4E3490).
pub fn parse_replay_position(input: &[u8]) -> i32 {
    let mut colon_count: i32 = 0;
    let mut accumulated: i32 = 0;
    let mut current: i32 = 0;
    let mut digit_count: i32 = 0;
    let mut frac_divisor: i32 = 0; // 0 = integer part, >0 = fractional
    let mut i = 0;

    loop {
        if i >= input.len() {
            return -1;
        }
        let ch = input[i];

        // Process digit runs
        while i < input.len() && input[i] >= b'0' && input[i] <= b'9' {
            let max_digits = if colon_count < 1 { 4 } else { 2 };
            if digit_count >= max_digits {
                return -1;
            }
            if frac_divisor == 0 {
                current = current * 10 + (input[i] - b'0') as i32;
                digit_count += 1;
            } else {
                let frac_value = ((input[i] - b'0') as i32 * 50) / frac_divisor;
                frac_divisor *= 10;
                accumulated += frac_value;
            }
            i += 1;
        }

        // Check delimiter
        if i >= input.len() {
            return -1;
        }
        let delim = input[i];

        if delim != b':' && delim != b'.' && delim != 0 {
            return -1;
        }

        if frac_divisor == 0 {
            // Integer part complete
            if colon_count > 0 && current > 59 {
                return -1;
            }
            accumulated = current + accumulated * 60;
            current = 0;
            digit_count = 0;

            if delim == b':' {
                if colon_count >= 2 {
                    return -1;
                }
                colon_count += 1;
            } else {
                // '.' or '\0' — convert seconds to frames
                accumulated *= 50;
            }
        } else {
            // Fractional part — ':' and '.' are invalid after '.'
            if delim == b':' || delim == b'.' {
                return -1;
            }
        }

        if delim == 0 {
            return accumulated;
        }

        if delim == b'.' {
            frac_divisor = 10;
        }

        i += 1;
    }
}
```

**Note:** This must match the original's behavior exactly. The decompilation shows
the algorithm accumulates digits, multiplies by 60 at each colon, and by 50 at
the final conversion. Fractional part: each digit contributes `(digit * 50) / 10^position`.

- [ ] **Step 4: Run tests**

```bash
cargo test -p openwa-core -- parse_replay
```

Expected: all pass. If any fail, adjust to match the original's exact logic.

- [ ] **Step 5: Commit**

```bash
git add crates/openwa-core/src/engine/replay.rs
git commit -m "feat: port ParseReplayPosition to Rust"
```

---

## Task 4: Hook Installation — Passthrough + ParseReplayPosition Replacement

**Files:**
- Create: `crates/openwa-wormkit/src/replacements/replay.rs`
- Modify: `crates/openwa-wormkit/src/replacements/mod.rs`

Install hooks for both functions. ReplayLoader starts as a passthrough (call
original, log params). ParseReplayPosition is fully replaced.

- [ ] **Step 1: Create replay.rs hook file**

```rust
// crates/openwa-wormkit/src/replacements/replay.rs

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::engine::replay;

static mut REPLAY_LOADER_ORIG: *const () = core::ptr::null();
static mut PARSE_POSITION_ORIG: *const () = core::ptr::null();

// ReplayLoader: stdcall(param_1: u32, mode: i32) -> u32. RET 0x8.
unsafe extern "stdcall" fn hook_replay_loader(param_1: u32, mode: i32) -> u32 {
    let _ = log_line(&format!(
        "[Replay] ReplayLoader param_1=0x{param_1:08X} mode={mode}"
    ));
    let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
        core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(param_1, mode);
    let _ = log_line(&format!(
        "[Replay] ReplayLoader returned 0x{result:08X}"
    ));
    result
}

// ParseReplayPosition: stdcall(input: *const u8) -> i32. RET 0x4.
unsafe extern "stdcall" fn hook_parse_replay_position(input: *const u8) -> i32 {
    // Build a safe slice from the C string
    let mut len = 0usize;
    while *input.add(len) != 0 {
        len += 1;
        if len > 256 { break; } // safety limit
    }
    let slice = core::slice::from_raw_parts(input, len + 1); // include null
    replay::parse_replay_position(slice)
}

pub fn install() -> Result<(), String> {
    unsafe {
        REPLAY_LOADER_ORIG = hook::install(
            "ReplayLoader",
            va::REPLAY_LOADER,
            hook_replay_loader as *const (),
        )? as *const ();

        // ParseReplayPosition — full replacement, trampoline stored but unused
        PARSE_POSITION_ORIG = hook::install(
            "ParseReplayPosition",
            va::PARSE_REPLAY_POSITION,
            hook_parse_replay_position as *const (),
        )? as *const ();
    }
    Ok(())
}
```

- [ ] **Step 2: Register in mod.rs**

In `crates/openwa-wormkit/src/replacements/mod.rs`, add:
- `mod replay;` in the module list
- `replay::install()?;` in `install_all()`

- [ ] **Step 3: Build**

```bash
cargo build --release -p openwa-wormkit
```

Expected: compiles successfully.

- [ ] **Step 4: Run headful replay test**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

Expected: game loads replay, plays through. Check `testdata/logs/openwa_latest.log`
for `[Replay] ReplayLoader` log lines confirming the hook fires and returns 0.

- [ ] **Step 5: Run headless replay test**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

Expected: PASS (byte-identical output). This confirms ParseReplayPosition's Rust
port produces the same frame numbers as the original.

- [ ] **Step 6: Commit**

```bash
git add crates/openwa-wormkit/src/replacements/replay.rs crates/openwa-wormkit/src/replacements/mod.rs
git commit -m "feat: hook ReplayLoader (passthrough) and replace ParseReplayPosition"
```

---

## Task 5: ReplayLoader Skeleton — Mode Routing + Header Parsing

**Files:**
- Modify: `crates/openwa-wormkit/src/replacements/replay.rs`
- Modify: `crates/openwa-core/src/engine/replay.rs`

Replace the passthrough with a mode-routing skeleton. Mode 1 (play) enters Rust;
other modes delegate to original. Start with header parsing only.

**Resource cleanup:** The original uses SEH try/catch to ensure fclose + free on
error. Our Rust replacement must use a guard pattern:

```rust
/// RAII guard for file handle and malloc'd buffer cleanup.
struct ReplayGuard {
    file: *mut FILE,
    payload: *mut u8,
}
impl Drop for ReplayGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.payload.is_null() {
                free(self.payload as *mut core::ffi::c_void);
            }
            if !self.file.is_null() {
                fclose(self.file);
            }
        }
    }
}
```

- [ ] **Step 1: Add CRT FFI declarations and ReplayGuard**

Add to `replay.rs` (wormkit crate) or a shared location:
- `extern "cdecl"` declarations for `fopen`, `fread`, `fclose`, `fseek`,
  `malloc`, `free`, `memset`, `memcpy`, `SetCurrentDirectoryA`
- `ReplayGuard` struct with `Drop` impl

- [ ] **Step 2: Implement mode routing + header**

```rust
unsafe extern "stdcall" fn hook_replay_loader(param_1: u32, mode: i32) -> u32 {
    // Modes 2, 3, 4: delegate to original
    if mode != 1 {
        let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
            core::mem::transmute(REPLAY_LOADER_ORIG);
        return orig(param_1, mode);
    }

    // Mode 1 (play): Rust implementation
    match replay_loader_play(param_1) {
        Ok(()) => 0,
        Err(e) => e as u32,
    }
}
```

Header parsing in `replay_loader_play`:
1. Check `DAT_0088c790 > 0x33` → return ArtClassLimit
2. Set `param_1+0xDB48 = 1`, `param_1+0xEF60 = 0`
3. `SetCurrentDirectoryA` to game data dir
4. `fopen(param_1+0xDB60, "rb")` → create ReplayGuard
5. Read 4 bytes: validate magic `0x4157`, extract version from upper 16 bits
6. Validate version ≤ 0x13
7. Store version at `param_1+0xDB50`, `param_1+0xDB54`, set `param_1+0xDB58 = 0xFFFFFFFF`
8. Read 4-byte payload size, validate against file size
9. malloc payload, fread into it → store in ReplayGuard

On any error, `ReplayGuard::drop` handles cleanup automatically.

- [ ] **Step 3: Run headful replay test**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

If game crashes, debug calling convention. If it loads but errors, check header
field offsets.

- [ ] **Step 4: Commit**

```bash
git add crates/openwa-wormkit/src/replacements/replay.rs crates/openwa-core/src/engine/replay.rs
git commit -m "feat: ReplayLoader mode routing + header parsing in Rust"
```

---

## Task 6: Version 2+ Payload Parsing — Scheme + Sub-version

**Files:**
- Modify: `crates/openwa-wormkit/src/replacements/replay.rs`
- Modify: `crates/openwa-core/src/engine/replay.rs`

Parse the first portion of the version 2+ payload: sub-version flags, game version
ID, and scheme data. Version 1 (legacy) delegates to original.

- [ ] **Step 1: Parse sub-version flags and format indicators**

After reading the payload into a `ReplayStream`:
1. Read secondary payload size + data
2. Parse sub-version flags, set globals (`DAT_0088AF54`, `DAT_0088AF41` etc.)
3. Read game version ID → `DAT_0088ABB0`, validate < 0x1F8
4. Read scheme presence flag → `DAT_0088AE0C`

- [ ] **Step 2: Parse scheme data**

If scheme present:
1. Read `SCHM` magic (4 bytes = `0x4D484353`), validate
2. Read scheme version byte (1, 2, or 3)
3. Call scheme reader helper via FFI bridge (`FUN_004613D0`)
4. For version 3: call `Scheme__ValidateExtendedOptions` via existing bridge
5. Copy scheme defaults from `SCHEME_V3_DEFAULTS` where needed

- [ ] **Step 3: Run headful + headless tests**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

- [ ] **Step 4: Commit**

```bash
git add crates/openwa-wormkit/src/replacements/replay.rs crates/openwa-core/src/engine/replay.rs
git commit -m "feat: ReplayLoader version 2+ scheme parsing"
```

---

## Task 7: Version 2+ Payload Parsing — Team Data

**Files:**
- Modify: `crates/openwa-wormkit/src/replacements/replay.rs`

Parse the team data section. This is the largest parsing section: 6 teams ×
0xD7B stride with per-worm names and weapon data blocks.

- [ ] **Step 1: Observer team loop**

Read observer/spectator player names (up to 0xD entries, stride 0x78) into global
buffers. Uses `read_prefixed_string` for names.

- [ ] **Step 2: Main team iteration**

For each of 6 team slots (stride 0xD7B at `0x878120`):
1. Read team flag byte — if zero, skip (empty slot)
2. Read team type byte, validate via `validate_team_type`
3. Read 5 sub-fields via stream helpers
4. Read worm alliance byte
5. Per-worm names: 8 worms × 0x11 bytes (fixed for version ≥ 10, else prefixed)
6. Worm count (validate 1-8), color, flag, grave, soundbank bytes
7. Team name (0x40 bytes), config name (0x40 bytes)
8. Weapon data: `memcpy` 0x400 + 0x154 + 0x400 + 0x300 bytes into global buffers

Write all data to the same global buffer addresses the original uses.

- [ ] **Step 3: Post-team validation**

1. Validate team count matches header
2. XOR integrity: `DAT_0088AF50 = payload_dword ^ 0xEF5B5C49`
3. Call `ProcessTeamColors` (0x466460, stdcall 1 param) via FFI
4. Call `Scheme__CheckWeaponLimits` via existing bridge

- [ ] **Step 4: Call remaining processing functions**

Bridge calls to:
- `ProcessSchemeDefaults` (0x4670F0, no params, plain RET)
- `ProcessReplayFlags` (0x467280, no params, plain RET)
- `ProcessAllianceData` (0x468890, no params, plain RET)
- `ValidateTeamSetup` (0x465E10)

- [ ] **Step 5: Run headful + headless tests**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

Headful: game must load and play correctly.
Headless: validates team/scheme data was written correctly.

- [ ] **Step 6: Commit**

```bash
git add crates/openwa-wormkit/src/replacements/replay.rs
git commit -m "feat: ReplayLoader team data parsing"
```

---

## Task 8: Post-Parse + Map Loading (Section 4)

**Files:**
- Modify: `crates/openwa-wormkit/src/replacements/replay.rs`

- [ ] **Step 1: Port post-parse logic**

1. Write payload to `data\playback.thm` via fopen/fwrite/fclose
2. Call map loader (`FUN_00490890`) via FFI bridge
3. Artclass index lookup via table at `0x6ACD38`
4. Version flag setup for compatibility

- [ ] **Step 2: Run headful + headless tests**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

- [ ] **Step 3: Commit**

```bash
git add crates/openwa-wormkit/src/replacements/replay.rs
git commit -m "feat: ReplayLoader post-parse and map loading"
```

---

## Task 9: Log Output (Section 5)

**Files:**
- Modify: `crates/openwa-wormkit/src/replacements/replay.rs`

Port the /getlog formatted output (~600 lines). This must match byte-for-byte
for headless replay testing.

**Strategy:** Use WA's own string resource and formatting functions via FFI.
Don't reimplement string resource loading or codepage conversion — bridge them.

- [ ] **Step 1: Port version/game info header lines**

1. Date/time via `_gmtime64` (CRT)
2. Version string lookup from tables at `0x6AB480` / `0x6AB7A4`
3. Format "Game:" and "Replay:" lines using `fprintf` or WA's format helpers
4. WA version line ("3.8.1")

- [ ] **Step 2: Port team listing**

1. Per-team line with flag letter (uppercased via codepage)
2. Team name in quotes, padded for alignment
3. Player name assignment
4. CPU teams: difficulty float
5. Human teams: host/first markers
6. Observer teams (negative team type)

- [ ] **Step 3: Run headless test after each sub-section**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

Compare output byte-for-byte. Fix any differences.

- [ ] **Step 4: Run full test suite**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

Both must pass.

- [ ] **Step 5: Commit**

```bash
git add crates/openwa-wormkit/src/replacements/replay.rs
git commit -m "feat: ReplayLoader log output formatting"
```

---

## Task 10: Version 1 Legacy Path (Optional/Deferred)

**Files:**
- Modify: `crates/openwa-wormkit/src/replacements/replay.rs`

Version 1 replays are from pre-3.5 WA. If encountered, the current implementation
would need to delegate to the original. This task ports the version 1 path.

**Deferred:** Only implement if a version 1 test replay is available.

- [ ] **Step 1: Find or create a version 1 test replay**
- [ ] **Step 2: Port version 1 parsing path**
- [ ] **Step 3: Test with version 1 replay**
- [ ] **Step 4: Commit**

---

---

## Verification Checklist

After Tasks 1-9:
- [ ] Headful replay test passes (all validations)
- [ ] Headless replay test passes (byte-identical output)
- [ ] `cargo test -p openwa-core -- replay` passes (unit tests)
- [ ] No regressions in existing tests
- [ ] ReplayLoader uses Rust path for mode 1 (play)
- [ ] ParseReplayPosition fully replaced (no trampoline calls)
- [ ] All replay helper functions named in Ghidra
