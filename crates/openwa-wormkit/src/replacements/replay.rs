//! Hooks for ReplayLoader and ParseReplayPosition.
//!
//! ReplayLoader (0x462DF0): stdcall(state, mode), RET 0x8.
//! ParseReplayPosition (0x4E3490): stdcall(input), RET 0x4.

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::engine::replay::{self, ReplayError};
use openwa_core::rebase::rb;

use core::ffi::c_void;
use core::ptr;

// ─── WA CRT function wrappers ────────────────────────────────────────────────
//
// WA.exe statically links MSVC 2005 CRT. We must call WA's own CRT functions
// via rebased addresses — NOT the Rust DLL's UCRT — because FILE* and heap
// state are CRT-instance-specific.

#[allow(non_camel_case_types)]
type FILE = c_void;

unsafe fn wa_fopen(filename: *const u8, mode: *const u8) -> *mut FILE {
    let f: unsafe extern "cdecl" fn(*const u8, *const u8) -> *mut FILE =
        core::mem::transmute(rb(0x005D_3271));
    f(filename, mode)
}

unsafe fn wa_fclose(stream: *mut FILE) {
    let f: unsafe extern "cdecl" fn(*mut FILE) = core::mem::transmute(rb(0x005D_399B));
    f(stream);
}

unsafe fn wa_fread(buf: *mut c_void, size: u32, count: u32, file: *mut FILE) -> u32 {
    let f: unsafe extern "cdecl" fn(*mut c_void, u32, u32, *mut FILE) -> u32 =
        core::mem::transmute(rb(0x005D_4531));
    f(buf, size, count, file)
}

unsafe fn wa_fwrite(buf: *const c_void, size: u32, count: u32, file: *mut FILE) -> u32 {
    let f: unsafe extern "cdecl" fn(*const c_void, u32, u32, *mut FILE) -> u32 =
        core::mem::transmute(rb(0x005D_3B76));
    f(buf, size, count, file)
}

unsafe fn wa_malloc(size: u32) -> *mut u8 {
    let f: unsafe extern "cdecl" fn(u32) -> *mut u8 =
        core::mem::transmute(rb(0x005D_0F65));
    f(size)
}

unsafe fn wa_free(ptr: *mut u8) {
    let f: unsafe extern "cdecl" fn(*mut u8) = core::mem::transmute(rb(0x005D_0D2B));
    f(ptr);
}

unsafe fn wa_fileno(stream: *mut FILE) -> i32 {
    let f: unsafe extern "cdecl" fn(*mut FILE) -> i32 =
        core::mem::transmute(rb(0x005D_5155));
    f(stream)
}

unsafe fn wa_filelengthi64(fd: i32) -> i64 {
    let f: unsafe extern "cdecl" fn(i32) -> i64 = core::mem::transmute(rb(0x005D_4FE1));
    f(fd)
}

extern "stdcall" {
    fn SetCurrentDirectoryA(path: *const u8) -> i32;
}

// ─── Trampoline storage ─────────────────────────────────────────────────────

static mut REPLAY_LOADER_ORIG: *const () = core::ptr::null();
#[allow(dead_code)]
static mut PARSE_POSITION_ORIG: *const () = core::ptr::null();

// ─── RAII cleanup guard ─────────────────────────────────────────────────────

struct ReplayGuard {
    file: *mut FILE,
    payload: *mut u8,
}

impl ReplayGuard {
    fn new() -> Self {
        Self {
            file: ptr::null_mut(),
            payload: ptr::null_mut(),
        }
    }
}

impl Drop for ReplayGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.payload.is_null() {
                wa_free(self.payload);
            }
            if !self.file.is_null() {
                wa_fclose(self.file);
            }
        }
    }
}

// ─── ReplayLoader hook ──────────────────────────────────────────────────────

unsafe extern "stdcall" fn hook_replay_loader(state: u32, mode: i32) -> u32 {
    if mode != 1 {
        let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
            core::mem::transmute(REPLAY_LOADER_ORIG);
        return orig(state, mode);
    }

    match replay_loader_play(state) {
        Ok(()) => 0u32,
        Err(e) => e as i32 as u32,
    }
}

// ─── Main replay loader (mode 1) ────────────────────────────────────────────

unsafe fn replay_loader_play(state: u32) -> Result<(), ReplayError> {
    let _ = log_line(&format!(
        "[Replay] ReplayLoader Rust: state=0x{state:08X}"
    ));

    // Guard: ArtClass counter (signed comparison, JL in asm)
    let artclass_counter = *(rb(va::G_ARTCLASS_COUNTER) as *const i32);
    if artclass_counter >= 0x34 {
        return Err(ReplayError::ArtClassLimit);
    }

    let s = state as *mut u8;
    *s.add(0xDB48) = 1;
    *(s.add(0xEF60) as *mut u32) = 0;

    // Open replay file
    SetCurrentDirectoryA(rb(0x0088_E078) as *const u8);
    let file = wa_fopen(s.add(0xDB60), b"rb\0".as_ptr());
    SetCurrentDirectoryA(rb(0x0088_E17D) as *const u8);

    if file.is_null() {
        return Err(ReplayError::FileNotFound);
    }

    let mut guard = ReplayGuard::new();
    guard.file = file;

    let fd = wa_fileno(file);
    let file_size = wa_filelengthi64(fd) as u64;

    // Read 4-byte header: lower 16 = magic, upper 16 = version
    let mut header: u32 = 0;
    if wa_fread(&mut header as *mut u32 as *mut c_void, 4, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }
    if (header & 0xFFFF) != replay::REPLAY_MAGIC as u32 {
        return Err(ReplayError::InvalidFormat);
    }

    let version = header >> 16;
    if version == 0 || version > 20 {
        return Err(ReplayError::VersionTooNew);
    }

    *(s.add(0xDB50) as *mut u32) = version;
    *(s.add(0xDB54) as *mut u32) = version;
    *(s.add(0xDB58) as *mut u32) = 0xFFFF_FFFF;

    // Read payload size + validate
    let mut payload_size: u32 = 0;
    if wa_fread(&mut payload_size as *mut u32 as *mut c_void, 4, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }
    if (payload_size as u64 + 8) > file_size {
        return Err(ReplayError::InvalidFormat);
    }

    // Read first payload
    let payload = wa_malloc(payload_size);
    if payload.is_null() {
        return Err(ReplayError::MallocFailure);
    }
    guard.payload = payload;
    if wa_fread(payload as *mut c_void, payload_size, 1, file) == 0 {
        return Err(ReplayError::FileNotFound);
    }

    // Handle first payload based on first dword (sub-version / content type)
    let first_dword = *(payload as *const i32);
    *(s.add(0xDB1C) as *mut i32) = first_dword;

    if first_dword >= 1 {
        let thm = wa_fopen(b"data\\playback.thm\0".as_ptr(), b"wb\0".as_ptr());
        if !thm.is_null() {
            wa_fwrite(payload as *const c_void, 1, payload_size, thm);
            wa_fclose(thm);
        }
    } else {
        *(s.add(0xDB20) as *mut i32) = *(payload.add(4) as *const i32);
        if first_dword >= -4 && first_dword < -2 {
            *(s.add(0xDB24) as *mut i32) = *(payload.add(8) as *const i32);
        } else if first_dword == -2 {
            let copy_len = payload_size.saturating_sub(8) as usize;
            if copy_len > 0x20 {
                return Err(ReplayError::InvalidFormat);
            }
            ptr::copy_nonoverlapping(payload.add(8), s.add(0xDB24), copy_len);
            *s.add(0xDB1C + payload_size as usize) = 0;
        }
    }

    // Free first payload (file stays open for version-specific second payload)
    wa_free(payload);
    guard.payload = ptr::null_mut();

    // Clear global buffers
    core::ptr::write_bytes(rb(va::G_TEAM_HEADER_DATA) as *mut u8, 0, 0x5728);
    core::ptr::write_bytes(rb(va::G_TEAM_SECONDARY_DATA) as *mut u8, 0, 0xD9DC);

    let _ = log_line(&format!(
        "[Replay] Header OK: ver={version} payload={payload_size} sub={first_dword}"
    ));

    // Dispatch to version-specific parsing
    if version == 1 {
        parse_version1(state, file, &mut guard)
    } else {
        parse_version2plus(state, version, file, file_size, payload_size, &mut guard)
    }
}

// ─── Version 1 (legacy) ─────────────────────────────────────────────────────

/// Version 1 replay parsing. Rare format (pre-3.5).
/// Currently delegates to original trampoline.
unsafe fn parse_version1(
    state: u32,
    _file: *mut FILE,
    guard: &mut ReplayGuard,
) -> Result<(), ReplayError> {
    let _ = log_line("[Replay] Version 1: delegating to original");

    // Close our file handle before original re-opens
    wa_fclose(guard.file);
    guard.file = ptr::null_mut();

    let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
        core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(state, 1);

    if result == 0 { Ok(()) } else { Err(ReplayError::FileNotFound) }
}

// ─── Version 2+ (modern) ────────────────────────────────────────────────────

/// Version 2+ replay parsing. Reads a second payload from the file containing
/// team/scheme/observer data, then parses it.
/// Currently delegates to original trampoline for the complex field parsing.
unsafe fn parse_version2plus(
    state: u32,
    version: u32,
    file: *mut FILE,
    file_size: u64,
    first_payload_size: u32,
    guard: &mut ReplayGuard,
) -> Result<(), ReplayError> {
    // Read second payload size (4 bytes from file, after first payload)
    let mut second_size: u32 = 0;
    if wa_fread(&mut second_size as *mut u32 as *mut c_void, 4, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }

    // Validate: header(4) + size1(4) + payload1 + size2(4) + payload2 <= file_size
    let total = 4u64 + 4 + first_payload_size as u64 + 4 + second_size as u64;
    if total > file_size {
        return Err(ReplayError::InvalidFormat);
    }

    // Read second payload
    let payload2 = wa_malloc(second_size);
    if payload2.is_null() {
        return Err(ReplayError::MallocFailure);
    }
    guard.payload = payload2;
    if wa_fread(payload2 as *mut c_void, second_size, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }

    let _ = log_line(&format!(
        "[Replay] Version {version}: second payload = {second_size} bytes"
    ));

    // Close file — no more reads needed
    wa_fclose(file);
    guard.file = ptr::null_mut();

    // TODO: Parse second payload using ReplayStream.
    // The second payload contains: sub-format flags, observer teams,
    // game version ID, scheme data (SCHM), per-team data (6 × 0xD7B stride),
    // weapon data blocks, alliance/seed data.
    //
    // For now, delegate to original which re-parses from scratch.

    // Free second payload before delegation
    wa_free(payload2);
    guard.payload = ptr::null_mut();

    let _ = log_line("[Replay] Delegating to original for field parsing");
    let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
        core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(state, 1);

    if result == 0 {
        let _ = log_line("[Replay] Original completed OK");
        Ok(())
    } else {
        let _ = log_line(&format!("[Replay] Original error: {}", result as i32));
        Err(ReplayError::FileNotFound)
    }
}

// ─── ParseReplayPosition full replacement ────────────────────────────────────

unsafe extern "stdcall" fn hook_parse_replay_position(input: *const u8) -> i32 {
    let mut len = 0usize;
    while *input.add(len) != 0 {
        len += 1;
        if len > 256 {
            break;
        }
    }
    let slice = core::slice::from_raw_parts(input, len + 1);
    replay::parse_replay_position(slice)
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        REPLAY_LOADER_ORIG = hook::install(
            "ReplayLoader",
            va::REPLAY_LOADER,
            hook_replay_loader as *const (),
        )? as *const ();

        PARSE_POSITION_ORIG = hook::install(
            "ParseReplayPosition",
            va::PARSE_REPLAY_POSITION,
            hook_parse_replay_position as *const (),
        )? as *const ();
    }
    Ok(())
}
