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

/// WA's fopen (0x5D3271)
unsafe fn wa_fopen(filename: *const u8, mode: *const u8) -> *mut FILE {
    let f: unsafe extern "cdecl" fn(*const u8, *const u8) -> *mut FILE =
        core::mem::transmute(rb(0x005D_3271));
    f(filename, mode)
}

/// WA's fclose (0x5D399B)
unsafe fn wa_fclose(stream: *mut FILE) {
    let f: unsafe extern "cdecl" fn(*mut FILE) = core::mem::transmute(rb(0x005D_399B));
    f(stream);
}

/// WA's fread wrapper (0x5D4531): fread(buf, size, count, file) → items read
unsafe fn wa_fread(buf: *mut c_void, size: u32, count: u32, file: *mut FILE) -> u32 {
    let f: unsafe extern "cdecl" fn(*mut c_void, u32, u32, *mut FILE) -> u32 =
        core::mem::transmute(rb(0x005D_4531));
    f(buf, size, count, file)
}

/// WA's _fwrite (0x5D3B76)
unsafe fn wa_fwrite(buf: *const c_void, size: u32, count: u32, file: *mut FILE) -> u32 {
    let f: unsafe extern "cdecl" fn(*const c_void, u32, u32, *mut FILE) -> u32 =
        core::mem::transmute(rb(0x005D_3B76));
    f(buf, size, count, file)
}

/// WA's malloc (0x5D0F65 → calls through to MSVC CRT malloc)
unsafe fn wa_malloc(size: u32) -> *mut u8 {
    let f: unsafe extern "cdecl" fn(u32) -> *mut u8 =
        core::mem::transmute(rb(0x005D_0F65));
    f(size)
}

/// WA's free (0x5D0D2B)
unsafe fn wa_free(ptr: *mut u8) {
    let f: unsafe extern "cdecl" fn(*mut u8) = core::mem::transmute(rb(0x005D_0D2B));
    f(ptr);
}

/// WA's _fileno (0x5D5155)
unsafe fn wa_fileno(stream: *mut FILE) -> i32 {
    let f: unsafe extern "cdecl" fn(*mut FILE) -> i32 =
        core::mem::transmute(rb(0x005D_5155));
    f(stream)
}

/// WA's _filelengthi64 (0x5D4FE1)
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

/// Ensures file handle and malloc'd buffer are freed on all exit paths.
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

// ─── ReplayLoader replacement ────────────────────────────────────────────────

/// ReplayLoader: stdcall(state: u32, mode: i32) -> u32. RET 0x8.
unsafe extern "stdcall" fn hook_replay_loader(state: u32, mode: i32) -> u32 {
    // Modes other than 1 (play): delegate to original
    if mode != 1 {
        let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
            core::mem::transmute(REPLAY_LOADER_ORIG);
        return orig(state, mode);
    }

    // Mode 1 (play): Rust implementation
    match replay_loader_play(state) {
        Ok(()) => 0u32,
        Err(e) => e as i32 as u32,
    }
}

/// Rust implementation of ReplayLoader mode 1 (play).
///
/// Currently: artclass guard + delegation to original.
/// Incrementally replacing sections (header, payload, version parsing).
unsafe fn replay_loader_play(state: u32) -> Result<(), ReplayError> {
    let _ = log_line(&format!(
        "[Replay] ReplayLoader Rust path: state=0x{state:08X}"
    ));

    // Guard: ArtClass counter. Assembly: CMP [0x88c790], 0x34; JL ok
    // JL = signed less-than. Counter can be -1 (0xFFFFFFFF) which passes.
    let artclass_counter = *(rb(va::G_ARTCLASS_COUNTER) as *const i32);
    if artclass_counter >= 0x34 {
        return Err(ReplayError::ArtClassLimit);
    }

    let s = state as *mut u8;

    // Set replay active flag and init counter (same as original)
    *s.add(0xDB48) = 1;
    *(s.add(0xEF60) as *mut u32) = 0;

    // ─── Header: open file, read magic, validate version ─────────────────

    SetCurrentDirectoryA(rb(0x0088_E078) as *const u8);
    let file = wa_fopen(s.add(0xDB60), b"rb\0".as_ptr());
    SetCurrentDirectoryA(rb(0x0088_E17D) as *const u8);

    if file.is_null() {
        return Err(ReplayError::FileNotFound);
    }

    let mut guard = ReplayGuard::new();
    guard.file = file;

    // File size
    let fd = wa_fileno(file);
    let file_size = wa_filelengthi64(fd) as u64;

    // Read 4-byte header
    let mut header: u32 = 0;
    if wa_fread(&mut header as *mut u32 as *mut c_void, 4, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }
    if (header & 0xFFFF) != replay::REPLAY_MAGIC as u32 {
        return Err(ReplayError::InvalidFormat);
    }

    // Version: upper 16 bits, valid 1..=20
    let version = header >> 16;
    if version == 0 || version > 20 {
        return Err(ReplayError::VersionTooNew);
    }

    *(s.add(0xDB50) as *mut u32) = version;
    *(s.add(0xDB54) as *mut u32) = version;
    *(s.add(0xDB58) as *mut u32) = 0xFFFF_FFFF;

    // Payload size
    let mut payload_size: u32 = 0;
    if wa_fread(&mut payload_size as *mut u32 as *mut c_void, 4, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }
    if (payload_size as u64 + 8) > file_size {
        return Err(ReplayError::InvalidFormat);
    }

    // Read payload
    let payload = wa_malloc(payload_size);
    if payload.is_null() {
        return Err(ReplayError::MallocFailure);
    }
    guard.payload = payload;

    if wa_fread(payload as *mut c_void, payload_size, 1, file) == 0 {
        return Err(ReplayError::FileNotFound);
    }

    // ─── First dword: sub-version / content type ─────────────────────────

    let first_dword = *(payload as *const i32);
    *(s.add(0xDB1C) as *mut i32) = first_dword;

    if first_dword >= 1 {
        // Write payload to playback.thm
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

    // Free payload and close file
    wa_free(payload);
    guard.payload = ptr::null_mut();
    wa_fclose(file);
    guard.file = ptr::null_mut();

    // ─── Clear global buffers ────────────────────────────────────────────

    core::ptr::write_bytes(rb(va::G_TEAM_HEADER_DATA) as *mut u8, 0, 0x5728);
    core::ptr::write_bytes(rb(va::G_TEAM_SECONDARY_DATA) as *mut u8, 0, 0xD9DC);

    let _ = log_line(&format!(
        "[Replay] Header OK: ver={version} payload={payload_size} sub={}",
        first_dword
    ));

    // ─── Delegate version-specific parsing to original ───────────────────
    //
    // The original re-parses from scratch. This is safe for read-only mode 1.
    // We've already set up DB48/EF60/DB50/DB54/DB58/DB1C and cleared globals,
    // which the original will overwrite with identical values.

    let _ = log_line("[Replay] Delegating to original for version-specific parsing");
    let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
        core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(state, 1);

    if result == 0 {
        Ok(())
    } else {
        let _ = log_line(&format!("[Replay] Original error: {}", result as i32));
        Err(ReplayError::FileNotFound)
    }
}

// ─── ParseReplayPosition full replacement ────────────────────────────────────

/// ParseReplayPosition: stdcall(input: *const u8) -> i32. RET 0x4.
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
