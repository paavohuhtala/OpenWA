//! Hooks for ReplayLoader and ParseReplayPosition.
//!
//! ReplayLoader (0x462DF0): stdcall(state, mode), RET 0x8.
//! ParseReplayPosition (0x4E3490): stdcall(input), RET 0x4.

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::engine::replay::{self, ReplayError, ReplayStream};
use openwa_core::rebase::rb;

use core::ffi::c_void;
use core::ptr;

// ─── WA CRT function wrappers ────────────────────────────────────────────────
//
// Must call WA's own CRT (MSVC 2005, statically linked) via rebased addresses,
// NOT the Rust DLL's UCRT — FILE* and heap are CRT-instance-specific.

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
        Self { file: ptr::null_mut(), payload: ptr::null_mut() }
    }
}

impl Drop for ReplayGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.payload.is_null() { wa_free(self.payload); }
            if !self.file.is_null() { wa_fclose(self.file); }
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
    // ArtClass counter guard (signed comparison, JL in asm)
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

    // Header: magic + version
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

    // First payload
    let mut payload_size: u32 = 0;
    if wa_fread(&mut payload_size as *mut u32 as *mut c_void, 4, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }
    if (payload_size as u64 + 8) > file_size {
        return Err(ReplayError::InvalidFormat);
    }
    let payload = wa_malloc(payload_size);
    if payload.is_null() {
        return Err(ReplayError::MallocFailure);
    }
    guard.payload = payload;
    if wa_fread(payload as *mut c_void, payload_size, 1, file) == 0 {
        return Err(ReplayError::FileNotFound);
    }

    // Handle first payload (sub-version / content type)
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
            if copy_len > 0x20 { return Err(ReplayError::InvalidFormat); }
            ptr::copy_nonoverlapping(payload.add(8), s.add(0xDB24), copy_len);
            *s.add(0xDB1C + payload_size as usize) = 0;
        }
    }

    wa_free(payload);
    guard.payload = ptr::null_mut();

    // Clear global buffers
    core::ptr::write_bytes(rb(va::G_TEAM_HEADER_DATA) as *mut u8, 0, 0x5728);
    core::ptr::write_bytes(rb(va::G_TEAM_SECONDARY_DATA) as *mut u8, 0, 0xD9DC);

    let _ = log_line(&format!(
        "[Replay] Header: ver={version} payload={payload_size} sub={first_dword}"
    ));

    // Dispatch to version-specific parsing
    if version == 1 {
        parse_version1(state, file, &mut guard)
    } else {
        parse_version2plus(state, version, file, file_size, payload_size, &mut guard)
    }
}

// ─── Version 1 (legacy) ─────────────────────────────────────────────────────

unsafe fn parse_version1(
    state: u32, _file: *mut FILE, guard: &mut ReplayGuard,
) -> Result<(), ReplayError> {
    let _ = log_line("[Replay] Version 1: delegating to original");
    wa_fclose(guard.file);
    guard.file = ptr::null_mut();
    delegate_to_original(state)
}

// ─── Version 2+ (modern) ────────────────────────────────────────────────────

unsafe fn parse_version2plus(
    state: u32, version: u32, file: *mut FILE,
    file_size: u64, first_payload_size: u32, guard: &mut ReplayGuard,
) -> Result<(), ReplayError> {
    // Read second payload
    let mut second_size: u32 = 0;
    if wa_fread(&mut second_size as *mut u32 as *mut c_void, 4, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }
    let total = 4u64 + 4 + first_payload_size as u64 + 4 + second_size as u64;
    if total > file_size {
        return Err(ReplayError::InvalidFormat);
    }
    let payload2 = wa_malloc(second_size);
    if payload2.is_null() {
        return Err(ReplayError::MallocFailure);
    }
    guard.payload = payload2;
    if wa_fread(payload2 as *mut c_void, second_size, 1, file) == 0 {
        return Err(ReplayError::InvalidFormat);
    }
    wa_fclose(file);
    guard.file = ptr::null_mut();

    // Parse second payload and write sub-format globals
    let data = core::slice::from_raw_parts(payload2, second_size as usize);
    let result = parse_and_populate_v2plus(state, data, version);

    wa_free(payload2);
    guard.payload = ptr::null_mut();

    match &result {
        Ok(info) => {
            let _ = log_line(&format!(
                "[Replay] Parsed: game_ver={} scheme_v={} teams={} observers={}",
                info.game_version_id, info.scheme_version, info.team_count, info.observer_count
            ));
        }
        Err(e) => {
            let _ = log_line(&format!("[Replay] Parse error: {e:?}"));
        }
    }

    // Delegate to original for remaining state population (scheme/team
    // globals, processing calls, map loading, log output).
    // The sub-format globals written above will be overwritten identically.
    delegate_to_original(state)
}

/// Delegate to original ReplayLoader trampoline (re-parses from scratch).
unsafe fn delegate_to_original(state: u32) -> Result<(), ReplayError> {
    let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
        core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(state, 1);
    if result == 0 { Ok(()) } else { Err(ReplayError::FileNotFound) }
}

// ─── Parsed replay info ─────────────────────────────────────────────────────

struct ReplayInfo {
    game_version_id: i32,
    scheme_version: u8,
    scheme_present: u8,
    team_count: u8,
    observer_count: u8,
}

// ─── Global buffer constants ─────────────────────────────────────────────────

/// Base address for per-team data in the global team buffer.
const TEAM_DATA_BASE: u32 = 0x0087_7FFC;
/// Stride between per-team entries in global buffer.
const TEAM_STRIDE: u32 = 0x0D7B;
/// Base for observer player entries (stride 0x78, 13 slots).
const OBSERVER_ENTRY_BASE: u32 = 0x0087_7A58;

// ─── Version 2+ payload parser ──────────────────────────────────────────────

/// Parse the second payload of a version 2+ replay file.
/// Writes parsed data to WA's global memory addresses.
unsafe fn parse_and_populate_v2plus(
    state: u32,
    data: &[u8],
    version: u32,
) -> Result<ReplayInfo, ReplayError> {
    let mut s = ReplayStream::new(data);

    // ─── Sub-format flags (version >= 10) ────────────────────────────────

    let mut obs_count: u16 = 0;

    if version >= 10 {
        let sub_format = s.read_u16()?;
        if sub_format != 0 {
            return Err(ReplayError::VersionTooNew);
        }

        if version >= 12 {
            if version < 18 {
                let _observer_mode = s.read_u8_validated(0, 2)?;
            } else {
                let _raw = s.read_u8_validated(0, 3)?;
            }
        }

        obs_count = s.read_u16_validated(1, version as u16)?;

        // Observer team loop: (4-byte ID, 1-byte type) until type == 0
        loop {
            let _team_id = s.read_u32()?;
            let obs_type = s.read_u8_validated(0, 2)?;
            if obs_type == 0 { break; }
        }
    }

    // ─── Game version ID ─────────────────────────────────────────────────

    let game_version_id = s.read_i32()?;
    if (game_version_id.wrapping_add(4) as u32) > 0x1F8 {
        return Err(ReplayError::VersionTooNew);
    }

    // Fixed names for old formats (game_ver < 10), prefixed for modern
    let use_fixed_names = game_version_id < 10;

    // ─── Scheme presence flag ────────────────────────────────────────────

    let scheme_present = s.read_u8_validated(1, 3)?;

    // Extra field for version 7-9 only (transitional)
    if version >= 7 && version <= 9 {
        let _extra = s.read_u32()?;
    }

    // ─── Scheme data ─────────────────────────────────────────────────────

    let mut scheme_version: u8 = 0;

    if scheme_present == 1 {
        // Scheme header byte (obs_count >= 3)
        if obs_count >= 3 {
            let _scheme_header = s.read_u8()?;
        }

        // Scheme size indicator (obs_count >= 0x14)
        let mut scheme_size_indicator: u32 = 0;
        if obs_count >= 0x14 {
            scheme_size_indicator = s.read_u32()?;
        }

        // SCHM magic + version byte
        let schm_magic = s.read_u32()?;
        scheme_version = s.read_u8()?;

        if schm_magic != 0x4D48_4353 {
            return Err(ReplayError::InvalidFormat);
        }

        // Scheme data: v1=216, v2=292, v3=variable
        let scheme_data_size = match scheme_version {
            1 => 0xD8_usize,
            2 => 0x124,
            3 => {
                if scheme_size_indicator < 0x12A || scheme_size_indicator > 0x197 {
                    return Err(ReplayError::InvalidFormat);
                }
                (scheme_size_indicator as usize) - 5
            }
            _ => return Err(ReplayError::VersionTooNew),
        };

        s.skip(scheme_data_size)?;

        // Random seed (u32)
        let _random_seed = s.read_u32()?;
    } else {
        // No scheme: terrain type byte (obs_count >= 13)
        if obs_count >= 13 {
            let _terrain_type = s.read_u8()?;
        }
    }

    // Map config bytes + replay name + host index
    let _map_byte1 = s.read_u8()?;
    let _map_byte2 = s.read_u8()?;

    let mut _replay_name = [0u8; 0x29];
    s.read_prefixed_string(&mut _replay_name)?;

    if version >= 9 {
        let _host_index = s.read_u8()?;
    }

    // ─── Observer player entries (13 slots) ──────────────────────────────

    let mut observer_count: u8 = 0;
    for _ in 0..13u32 {
        let flag = s.read_u8()?;
        if flag == 0 { continue; }
        observer_count += 1;

        let mut _name = [0u8; 0x11];
        s.read_prefixed_string(&mut _name)?;
        let mut _display = [0u8; 0x31];
        s.read_prefixed_string(&mut _display)?;
        let mut _config = [0u8; 0x29];
        s.read_prefixed_string(&mut _config)?;

        let _u16_field = s.read_u16()?;
        let _byte1 = s.read_u8()?;
        let _u32_field = s.read_u32()?;
        let _byte2 = s.read_u8()?;
    }

    // ─── XOR game ID (obs_count >= 16) ──────────────────────────────────

    if obs_count >= 16 {
        let xor_a = s.read_u32()?;
        let _xor_b = s.read_u32()?;
        let _game_id = xor_a ^ replay::REPLAY_XOR_KEY;
    }

    // ─── Team entries (up to 6, stride 0xD7B in global buffer) ───────────

    let mut team_count: u8 = 0;

    for _team_idx in 0..6u32 {
        // Team flag: 0 = empty slot, non-zero = active team
        let team_flag = s.read_u8()?;
        if team_flag == 0 {
            continue;
        }
        team_count += 1;

        // Per-team structure (traced from hex dump + decompilation):
        // type, alliance(0-5), unk_byte, pre-name(prefixed), 8×worm_name,
        // worm_count, team_name(prefixed), [extra if obs>13], config_name,
        // worm_count2, color, flag, grave, soundbank_flag, soundbank

        let team_type = s.read_u8()? as i8;
        if !replay::validate_team_type(team_type) {
            return Err(ReplayError::InvalidFormat);
        }

        let _alliance = s.read_u8_validated(0, 5)?;
        let _unk_byte = s.read_u8()?;

        // Pre-loop name field (ReadWormName in decompilation)
        // For bots.WAgame this reads "CPU 2" / "CPU 1" — NOT the team name
        let mut _pre_name = [0u8; 0x11];
        s.read_worm_name(&mut _pre_name, use_fixed_names)?;

        // 8 worm names
        for _worm_idx in 0..8u32 {
            let mut _worm_name = [0u8; 0x11];
            s.read_worm_name(&mut _worm_name, use_fixed_names)?;
        }

        // Worm count byte 1 (DAT_00878092) — stored WITHOUT validation
        let _worm_count_raw = s.read_u8()?;

        // Team name (prefixed string, max 0x40)
        let mut _team_name = [0u8; 0x41];
        s.read_prefixed_string(&mut _team_name)?;

        // Extra byte if obs_count > 13
        if obs_count > 13 {
            let _extra = s.read_u8()?;
        }

        // Config/country name (prefixed string, max 0x40)
        let mut _config_name = [0u8; 0x41];
        s.read_prefixed_string(&mut _config_name)?;

        // Worm count byte 2 (DAT_00878094) — validated 1-8
        // Assembly: if (7 < (value - 1) unsigned) throw
        let _worm_count = s.read_u8()?;
        if _worm_count == 0 || _worm_count > 8 {
            return Err(ReplayError::InvalidFormat);
        }

        // Color, flag, grave, soundbank_flag, soundbank
        let _color = s.read_u8()?;
        let _flag_byte = s.read_u8()?;
        let _grave = s.read_u8()?;
        let _soundbank_flag = s.read_u8()?;
        let _soundbank = s.read_u8()?;

        // Weapon data blocks
        s.skip(0x400)?; // weapon ammo (1024 bytes)
        s.skip(0x154)?; // weapon delay (340 bytes)
        s.skip(0x400)?; // weapon ammo 2 (1024 bytes)
        s.skip(0x300)?; // weapon data 3 (768 bytes)
    }

    if team_count == 0 {
        return Err(ReplayError::InvalidFormat);
    }

    // ─── Post-team processing data ───────────────────────────────────────

    if scheme_present == 1 {
        // Map seed u16
        let _map_seed = s.read_u16()?;

        // Additional reads depend on map_seed value and various globals.
        // These involve conditional per-team weapon config reads that use
        // globals set by ProcessTeamColors (which we haven't called).
        // Skip remaining bytes — the original handles all of this.
    } else {
        // No scheme: read random seed + alliance data
        let _random_seed = s.read_u32()?;
        let _alliance_count = s.read_u8()?;
        // Per-alliance prefixed strings follow...
    }

    let _ = log_line(&format!(
        "[Replay] Post-team: cursor={} remaining={}", s.cursor(), s.remaining()
    ));

    Ok(ReplayInfo {
        game_version_id,
        scheme_version,
        scheme_present,
        team_count,
        observer_count,
    })
}

// ─── ParseReplayPosition full replacement ────────────────────────────────────

unsafe extern "stdcall" fn hook_parse_replay_position(input: *const u8) -> i32 {
    let mut len = 0usize;
    while *input.add(len) != 0 {
        len += 1;
        if len > 256 { break; }
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
