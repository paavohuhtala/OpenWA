//! Hooks for ReplayLoader and ParseReplayPosition.
//!
//! ReplayLoader (0x462DF0): stdcall(state, mode), RET 0x8.
//! ParseReplayPosition (0x4E3490): stdcall(input), RET 0x4.

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::engine::game_info::GameInfo;
use openwa_core::frontend::MapView;
use openwa_core::engine::replay::{self, ReplayError, ReplayStream, ReplayTeamEntry};
use openwa_core::rebase::rb;
use openwa_core::wa_alloc::{wa_free, wa_malloc};

use core::ffi::c_void;
use core::fmt::Write;
use core::ptr;
use std::ffi::CStr;
use std::fs::File;
use std::io::Read;
use std::mem::ManuallyDrop;
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use std::path::PathBuf;

// ─── WA CRT FILE* conversion ────────────────────────────────────────────────
//
// WA's CRT FILE* can't be used with Rust's std::fs::File directly, but we can
// extract the Win32 HANDLE via _fileno + _get_osfhandle and wrap it.

#[allow(non_camel_case_types)]
type FILE = c_void;

/// Convert a WA CRT FILE* to a borrowed Rust File (ManuallyDrop — we don't own it).
unsafe fn wa_file_to_rust(file: *mut FILE) -> Option<ManuallyDrop<File>> {
    let fileno: unsafe extern "cdecl" fn(*mut FILE) -> i32 =
        core::mem::transmute(rb(va::WA_FILENO));
    let get_osfhandle: unsafe extern "cdecl" fn(i32) -> isize =
        core::mem::transmute(rb(va::WA_GET_OSFHANDLE));

    let fd = fileno(file);
    if fd < 0 { return None; }
    let handle = get_osfhandle(fd);
    if handle == -1 || handle == -2 { return None; }
    Some(ManuallyDrop::new(File::from_raw_handle(handle as *mut core::ffi::c_void)))
}

/// Load a WA string resource by ID (stdcall). Returns null-terminated C string.
unsafe fn wa_load_string(id: u32) -> *const u8 {
    let f: unsafe extern "stdcall" fn(u32) -> *const u8 =
        core::mem::transmute(rb(va::WA_LOAD_STRING));
    f(id)
}

// ─── Trampoline storage ─────────────────────────────────────────────────────

static mut REPLAY_LOADER_ORIG: *const () = core::ptr::null();
#[allow(dead_code)]
static mut PARSE_POSITION_ORIG: *const () = core::ptr::null();
/// Replay timestamp extracted during parsing (time_t as u32).
static mut REPLAY_TIMESTAMP: u32 = 0;

// ─── RAII cleanup guard (for wa_malloc'd payload) ───────────────────────────

struct PayloadGuard { payload: *mut u8 }
impl Drop for PayloadGuard {
    fn drop(&mut self) {
        unsafe { if !self.payload.is_null() { wa_free(self.payload); } }
    }
}

// ─── ReplayLoader hook ──────────────────────────────────────────────────────

unsafe extern "stdcall" fn hook_replay_loader(state: *mut GameInfo, mode: i32) -> u32 {
    if mode != 1 {
        let orig: unsafe extern "stdcall" fn(*mut GameInfo, i32) -> u32 =
            core::mem::transmute(REPLAY_LOADER_ORIG);
        return orig(state, mode);
    }
    match replay_loader_play(state) {
        Ok(()) => 0u32,
        Err(e) => e as i32 as u32,
    }
}

// ─── Main replay loader (mode 1) ────────────────────────────────────────────

unsafe fn replay_loader_play(gi: *mut GameInfo) -> Result<(), ReplayError> {
    let artclass_counter = *(rb(va::G_ARTCLASS_COUNTER) as *const i32);
    if artclass_counter >= 0x34 { return Err(ReplayError::ArtClassLimit); }
    (*gi).replay_active = 1;
    (*gi).replay_field_ef60 = 0;

    // Open replay file using Rust's std::fs::File
    let replay_path = {
        let data_dir = CStr::from_ptr(rb(va::G_DATA_DIR) as *const i8);
        let file_name = CStr::from_ptr((*gi).replay_filename.as_ptr() as *const i8);
        let dir = data_dir.to_str().unwrap_or(".");
        let name = file_name.to_str().unwrap_or("");
        if Path::new(name).is_absolute() {
            PathBuf::from(name)
        } else {
            Path::new(dir).join(name)
        }
    };

    let mut file = File::open(&replay_path)
        .map_err(|_| ReplayError::FileNotFound)?;
    let file_size = file.metadata().map_err(|_| ReplayError::FileNotFound)?.len();

    // Helper: read exact bytes from file into a typed value
    fn read_u32(f: &mut File) -> Result<u32, ReplayError> {
        let mut buf = [0u8; 4];
        f.read_exact(&mut buf).map_err(|_| ReplayError::InvalidFormat)?;
        Ok(u32::from_le_bytes(buf))
    }

    // Header
    let header = read_u32(&mut file)?;
    if (header & 0xFFFF) != replay::REPLAY_MAGIC as u32 { return Err(ReplayError::InvalidFormat); }
    let version = header >> 16;
    if version == 0 || version > 20 { return Err(ReplayError::VersionTooNew); }

    (*gi).replay_format_version = version;
    (*gi).replay_format_version_2 = version;
    (*gi).replay_field_db58 = 0xFFFFFFFF;

    // First payload
    let payload_size = read_u32(&mut file)?;
    if (payload_size as u64 + 8) > file_size { return Err(ReplayError::InvalidFormat); }
    let payload = wa_malloc(payload_size);
    if payload.is_null() { return Err(ReplayError::MallocFailure); }
    let mut guard = PayloadGuard { payload };
    let payload_slice = core::slice::from_raw_parts_mut(payload, payload_size as usize);
    file.read_exact(payload_slice).map_err(|_| ReplayError::FileNotFound)?;

    let first_dword = *(payload as *const i32);
    (*gi).replay_map_type = first_dword;
    if first_dword >= 1 {
        // Write payload to playback.thm using Rust File
        let _ = std::fs::write("data\\playback.thm", payload_slice);
    } else {
        (*gi).replay_payload_2 = *(payload.add(4) as *const i32);
        if first_dword >= -4 && first_dword < -2 {
            *((*gi).replay_payload_extra.as_mut_ptr() as *mut i32) =
                *(payload.add(8) as *const i32);
        } else if first_dword == -2 {
            let n = payload_size.saturating_sub(8) as usize;
            if n > 0x20 { return Err(ReplayError::InvalidFormat); }
            ptr::copy_nonoverlapping(
                payload.add(8),
                (*gi).replay_payload_extra.as_mut_ptr(),
                n,
            );
            // Null-terminate: write 0 at replay_map_type + payload_size offset
            // (the original uses base+0xDB1C+payload_size which spans into replay_payload_extra)
            *((gi as *mut u8).add(0xDB1C + payload_size as usize)) = 0;
        }
    }
    wa_free(payload); guard.payload = ptr::null_mut();

    // Clear global buffers
    ptr::write_bytes(rb(va::G_TEAM_HEADER_DATA) as *mut u8, 0, 0x5728);
    ptr::write_bytes(rb(va::G_TEAM_SECONDARY_DATA) as *mut u8, 0, 0xD9DC);

    let _ = log_line(&format!("[Replay] Header: ver={version} payload={payload_size} sub={first_dword}"));

    if version == 1 {
        drop(file);
        return parse_version1(gi);
    }

    // ─── Version 2+: read second payload ─────────────────────────────────

    let second_size = read_u32(&mut file)?;
    if (4u64 + 4 + payload_size as u64 + 4 + second_size as u64) > file_size { return Err(ReplayError::InvalidFormat); }
    let p2 = wa_malloc(second_size);
    if p2.is_null() { return Err(ReplayError::MallocFailure); }
    guard.payload = p2;
    let p2_slice = core::slice::from_raw_parts_mut(p2, second_size as usize);
    file.read_exact(p2_slice).map_err(|_| ReplayError::InvalidFormat)?;
    drop(file); // Done reading — close replay file

    // Parse and write to globals
    let data = core::slice::from_raw_parts(p2, second_size as usize);
    let result = parse_and_write_v2plus(gi, data, version);
    wa_free(p2); guard.payload = ptr::null_mut();

    match result {
        Ok(()) => {
            call_usercall_eax(gi, rb(va::REPLAY_PROCESS_FLAGS));
            let wa_log_file = *(rb(va::G_LOG_FILE_PTR) as *const *mut FILE);
            if !wa_log_file.is_null() {
                if let Some(mut log_file) = wa_file_to_rust(wa_log_file) {
                    write_replay_log(gi, &mut *log_file)?;
                }
            }
            let _ = log_line("[Replay] Rust replay loading complete");
            Ok(())
        }
        Err(e) => {
            let _ = log_line(&format!("[Replay] Parse failed ({e:?}), falling back to original"));
            delegate_to_original(gi)
        }
    }
}

unsafe fn parse_version1(gi: *mut GameInfo) -> Result<(), ReplayError> {
    delegate_to_original(gi)
}

unsafe fn delegate_to_original(gi: *mut GameInfo) -> Result<(), ReplayError> {
    let orig: unsafe extern "stdcall" fn(*mut GameInfo, i32) -> u32 = core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(gi, 1);
    if result == 0 { Ok(()) } else { Err(ReplayError::FileNotFound) }
}

// ─── Version 2+ parser that writes to globals ───────────────────────────────

/// Helper: write byte to rebased global address.
#[inline]
unsafe fn wb(addr: u32, val: u8) { *(rb(addr) as *mut u8) = val; }
/// Helper: write u32 to rebased global address.
#[inline]
unsafe fn wd(addr: u32, val: u32) { *(rb(addr) as *mut u32) = val; }

unsafe fn parse_and_write_v2plus(
    gi: *mut GameInfo, data: &[u8], version: u32,
) -> Result<(), ReplayError> {
    let mut s = ReplayStream::new(data);

    // ── Sub-format flags ─────────────────────────────────────────────────
    let ver_gt7 = (version > 7) as u8;
    wb(va::G_REPLAY_VER_FLAG_A, ver_gt7);
    wb(va::G_REPLAY_VER_FLAG_B, ver_gt7);
    wd(va::G_REPLAY_SUB_FORMAT, 0);

    let mut obs_count: u16 = 0;

    if version >= 10 {
        let sub_format = s.read_u16()?;
        wd(va::G_REPLAY_SUB_FORMAT, sub_format as u32);
        if sub_format != 0 { return Err(ReplayError::VersionTooNew); }

        if version >= 12 {
            if version < 18 {
                let mode = s.read_u8_validated(0, 2)?;
                wb(va::G_REPLAY_GAME_MODE, mode);
            } else {
                let raw = s.read_u8_validated(0, 3)?;
                if raw >= 2 && raw <= 3 {
                    wb(va::G_REPLAY_GAME_MODE, raw - 1);
                    wb(va::G_REPLAY_VER_FLAG_A, 1); wb(va::G_REPLAY_VER_FLAG_B, 1);
                } else {
                    wb(va::G_REPLAY_VER_FLAG_A, (raw != 0) as u8);
                    wb(va::G_REPLAY_GAME_MODE, 0);
                    wb(va::G_REPLAY_VER_FLAG_B, (raw != 0) as u8);
                }
            }
        }

        obs_count = s.read_u16_validated(1, version as u16)?;
        wd(va::G_OBSERVER_COUNT, obs_count as u32);

        // FUN_0053ee00: cleanup observer array. usercall(ESI=0x88C35C). Plain RET.
        call_usercall_esi(rb(va::G_OBSERVER_ARRAY), rb(va::REPLAY_CLEANUP_OBSERVERS));

        // Observer team loop: register each observer via naked bridge
        loop {
            let team_id = s.read_u32()?;
            let obs_type = s.read_u8_validated(0, 2)?;
            // Build 4-DWORD struct: [team_id, 0, obs_type_as_u32, ???]
            // Original: aiStack_260[0x2d]=team_id, [0x2e]=0, [0x2f]=obs_type
            let obs_data: [u32; 4] = [team_id, 0, obs_type as u32, 0];
            // RegisterObserver: usercall(ESI=0x88C35C) + stdcall(1 param = &data). RET 0x4.
            call_register_observer(
                rb(va::G_OBSERVER_ARRAY), obs_data.as_ptr() as u32, rb(va::REPLAY_REGISTER_OBSERVER),
            );
            if obs_type == 0 {
                // The terminating entry's team_id is the replay timestamp (time_t)
                REPLAY_TIMESTAMP = team_id;
                break;
            }
        }
    }

    let game_version_id = s.read_i32()?;
    wd(va::G_REPLAY_VERSION_ID, game_version_id as u32);
    if (game_version_id.wrapping_add(4) as u32) > 0x1F8 { return Err(ReplayError::VersionTooNew); }
    let use_fixed_names = game_version_id < 10;

    // ── Scheme presence ──────────────────────────────────────────────────
    let scheme_present = s.read_u8_validated(1, 3)?;
    wd(va::G_REPLAY_SCHEME_PRESENT, scheme_present as u32);

    // Extra field for version 7-9 only
    if version >= 7 && version <= 9 {
        let _extra = s.read_u32()?;
    }

    if scheme_present == 1 {
        // Scheme header byte
        if obs_count >= 3 {
            let header_byte = s.read_u8()?;
            wb(va::G_SCHEME_HEADER, header_byte);
            // If header >= 0 (signed): load built-in scheme from resources
            if (header_byte as i8) >= 0 {
                // FUN_004D4840: stdcall(2 params). RET 0x8.
                let f: unsafe extern "stdcall" fn(u32, i32) =
                    core::mem::transmute(rb(va::SCHEME_LOAD_BUILTIN));
                f(rb(va::G_SCHEME_DEST), header_byte as i32);
            }
        }

        // Scheme size indicator
        let mut scheme_size_indicator: u32 = 0;
        if obs_count >= 0x14 {
            scheme_size_indicator = s.read_u32()?;
        }

        // SCHM magic + version
        let magic = s.read_u32()?;
        let scheme_version = s.read_u8()?;
        if magic != 0x4D484353 { return Err(ReplayError::InvalidFormat); }

        let scheme_data_size = match scheme_version {
            1 => 0xD8_usize,
            2 => 0x124,
            3 => {
                if scheme_size_indicator < 0x12A || scheme_size_indicator > 0x197 {
                    return Err(ReplayError::InvalidFormat);
                }
                // Copy defaults first for v3
                ptr::copy_nonoverlapping(
                    rb(va::SCHEME_V3_DEFAULTS) as *const u8,
                    rb(va::G_SCHEME_V3_DATA) as *mut u8, 0x6E,
                );
                (scheme_size_indicator as usize) - 5
            }
            _ => return Err(ReplayError::VersionTooNew),
        };

        // Copy scheme data from stream to global 0x88DAE0
        let scheme_slice = s.advance_raw(scheme_data_size)?;
        ptr::copy_nonoverlapping(scheme_slice.as_ptr(), rb(va::G_SCHEME_DATA) as *mut u8, scheme_data_size);

        let header_val = *(rb(va::G_SCHEME_HEADER) as *const i8);

        // If scheme_header < 0 (signed): apply defaults.
        // V1: clear super weapons (0x4C bytes) + copy V3 defaults.
        // V2: copy V3 defaults ONLY (super weapon data is already in the payload).
        if header_val < 0 {
            if scheme_version == 1 {
                ptr::write_bytes(rb(va::G_SCHEME_OPTIONS) as *mut u8, 0, 0x4C);
            }
            if scheme_version <= 2 {
                ptr::copy_nonoverlapping(
                    rb(va::SCHEME_V3_DEFAULTS) as *const u8,
                    rb(va::G_SCHEME_V3_DATA) as *mut u8, 0x6E,
                );
            }
        }

        // Validate extended options for v3
        if scheme_version == 3 {
            let validate: unsafe extern "cdecl" fn() -> i32 =
                core::mem::transmute(rb(va::SCHEME_VALIDATE_EXTENDED));
            let r = validate();
            if r != 0 { return Err(ReplayError::InvalidFormat); }
        }

        // Random seed: the stream value REPLACES the current seed.
        // The decompiler misleadingly shows save/restore, but ReadU32
        // overwrites the save destination (same EBP+0x48 local).
        // Assembly: MOV [0x88D0B4], stream_value
        let seed_from_stream = s.read_u32()?;
        wd(va::G_RANDOM_SEED, seed_from_stream);
    } else {
        // No scheme path — fall back to delegation for now
        // (ProcessAllianceData reads from stream via usercall EAX)
        let _ = log_line("[Replay] No-scheme path: delegating");
        return Err(ReplayError::InvalidFormat); // triggers fallback
    }

    let map_byte1 = s.read_u8()?;
    let map_byte2 = s.read_u8()?;
    wb(va::G_MAP_BYTE_1, map_byte1);
    wb(va::G_MAP_BYTE_2, map_byte2);

    let mut replay_name = [0u8; 0x29];
    s.read_prefixed_string(&mut replay_name)?;
    ptr::copy_nonoverlapping(replay_name.as_ptr(), rb(va::G_REPLAY_NAME) as *mut u8, 0x29);

    if version >= 9 {
        let host = s.read_u8()?;
        // Store as low byte of DAT_008779E0 (CONCAT31 pattern)
        let current = *(rb(va::G_HOST_PLAYER) as *const u32);
        wd(va::G_HOST_PLAYER, (current & 0xFFFFFF00) | host as u32);
    } else {
        wd(va::G_HOST_PLAYER, 0xFFFFFFFF);
    }

    let mut player_count: u8 = 0;
    for i in 0..13u32 {
        let base = va::G_PLAYER_ARRAY + 0x74 + i * 0x78; // player[i].flag
        let flag = s.read_u8()?;
        wb(base, flag);
        if flag == 0 { continue; }

        // NOTE: original checks if i == host_player_index here (sets a local flag, unused)
        player_count += 1;

        let mut name = [0u8; 0x11];
        s.read_prefixed_string(&mut name)?;
        ptr::copy_nonoverlapping(name.as_ptr(), rb(va::G_PLAYER_ARRAY + i * 0x78) as *mut u8, 0x11);

        let mut display = [0u8; 0x31];
        s.read_prefixed_string(&mut display)?;
        ptr::copy_nonoverlapping(display.as_ptr(), rb(va::G_PLAYER_ARRAY + 0x11 + i * 0x78) as *mut u8, 0x31);

        let mut config = [0u8; 0x29];
        s.read_prefixed_string(&mut config)?;
        ptr::copy_nonoverlapping(config.as_ptr(), rb(va::G_PLAYER_ARRAY + 0x42 + i * 0x78) as *mut u8, 0x29);

        let u16_val = s.read_u16()?;
        // Ghidra shows (&DAT[i*0x3C]) with u16 elements → byte offset = i*0x3C*2 = i*0x78
        *(rb(va::G_PLAYER_ARRAY + 0x6C + i * 0x78) as *mut u16) = u16_val;

        let byte1 = s.read_u8()?;
        wb(va::G_PLAYER_ARRAY + 0x6E + i * 0x78, byte1);

        let u32_val = s.read_u32()?;
        // Ghidra shows (&DAT[i*0x1E]) with u32 elements → byte offset = i*0x1E*4 = i*0x78
        wd(va::G_PLAYER_ARRAY + 0x70 + i * 0x78, u32_val);

        let byte2 = s.read_u8()?;
        wb(va::G_PLAYER_ARRAY + 0x77 + i * 0x78, byte2);
    }
    wb(va::G_PLAYER_COUNT, player_count);

    if obs_count >= 16 {
        let xor_a = s.read_u32()?;
        let _xor_b = s.read_u32()?;
        wd(va::G_REPLAY_GAME_ID, xor_a ^ replay::REPLAY_XOR_KEY);
    }

    let teams = rb(va::G_TEAM_DATA) as *mut ReplayTeamEntry;
    let mut team_count: u8 = 0;
    for team_idx in 0..6usize {
        let team = &mut *teams.add(team_idx);
        let team_flag = s.read_u8()?;
        team.flag = team_flag;
        if team_flag == 0 { continue; }
        team_count += 1;
    
        let team_type = s.read_u8()? as i8;
        if !replay::validate_team_type(team_type) { return Err(ReplayError::InvalidFormat); }
        team.team_type = team_type as u8;
        team.alliance = s.read_u8_validated(0, 5)?;
        team.unknown_02 = s.read_u8()?;

        // Config abbreviation / pre-loop worm name
        s.read_worm_name(&mut team.config_abbrev, use_fixed_names)?;

        // 8 worm names (stored in separate global array, not in this struct)
        for worm_idx in 0..8u32 {
            let name_off = ((team_idx) * 0xCB + worm_idx as usize) * 0x11;
            let dest = rb(va::G_WORM_NAMES) as *mut u8;
            if use_fixed_names {
                let slice = s.advance_raw(0x11)?;
                ptr::copy_nonoverlapping(slice.as_ptr(), dest.add(name_off), 0x11);
            } else {
                let mut name = [0u8; 0x11];
                s.read_prefixed_string(&mut name)?;
                ptr::copy_nonoverlapping(name.as_ptr(), dest.add(name_off), 0x11);
            }
        }

        team.worm_count_raw = s.read_u8()?;
        s.read_prefixed_string(&mut team.team_name)?;

        if obs_count > 13 {
            team.extra_byte = s.read_u8()?;
        }

        s.read_prefixed_string(&mut team.config_name)?;

        let worm_count = s.read_u8()?;
        if worm_count == 0 || worm_count > 8 { return Err(ReplayError::InvalidFormat); }
        team.worm_count = worm_count;
        team.color = s.read_u8()?;
        team.flag2 = s.read_u8()?;
        team.grave = s.read_u8()?;
        team.soundbank = s.read_u8()?;
        team.soundbank_extra = s.read_u8()?;

        // Weapon data: 4 contiguous blocks copied into weapons[0xC54]
        let w1 = s.advance_raw(0x400)?;
        ptr::copy_nonoverlapping(w1.as_ptr(), team.weapons.as_mut_ptr(), 0x400);
        let w2 = s.advance_raw(0x154)?;
        ptr::copy_nonoverlapping(w2.as_ptr(), team.weapons.as_mut_ptr().add(0x400), 0x154);
        let w3 = s.advance_raw(0x400)?;
        ptr::copy_nonoverlapping(w3.as_ptr(), team.weapons.as_mut_ptr().add(0x554), 0x400);
        let w4 = s.advance_raw(0x300)?;
        ptr::copy_nonoverlapping(w4.as_ptr(), team.weapons.as_mut_ptr().add(0x954), 0x300);
    }

    if team_count == 0 { return Err(ReplayError::InvalidFormat); }

    wb(va::G_TEAM_COUNT, team_count);

    // ProcessTeamColors: stdcall(1 param = game_info). RET 0x4.
    let process_colors: unsafe extern "stdcall" fn(*mut GameInfo) =
        core::mem::transmute(rb(va::REPLAY_PROCESS_TEAM_COLORS));
    process_colors(gi);

    let map_seed = s.read_u16()?;
    wd(va::G_MAP_SEED, map_seed as u32);

    // FUN_0045d640: stdcall(1 param = game_info). 1032-line function.
    let fun_45d640: unsafe extern "stdcall" fn(*mut GameInfo) =
        core::mem::transmute(rb(va::REPLAY_PROCESS_STATE));
    fun_45d640(gi);

    if map_seed == 0 || map_seed == 0xFFFF {
        call_process_scheme_defaults(gi, rb(va::REPLAY_PROCESS_SCHEME_DEFAULTS));
    }
    if map_seed != 0 {
        // Per-team weapon config reads from stream — not yet implemented in Rust.
        // Fall back to the original parser which handles this from scratch.
        let _ = log_line(&format!("[Replay] Non-zero map_seed (0x{:04X}), delegating to original", map_seed));
        return Err(ReplayError::InvalidFormat); // triggers delegate_to_original
    }

    // ValidateTeamSetup: stdcall(1 param = game_info)
    let validate_setup: unsafe extern "stdcall" fn(*mut GameInfo) =
        core::mem::transmute(rb(va::REPLAY_VALIDATE_TEAM_SETUP));
    validate_setup(gi);

    // Assembly: MOV ESI,[seed]; PUSH ESI; CALL srand; rand(); SHL<<16; rand(); ADD
    let current_seed = *(rb(va::G_RANDOM_SEED) as *const u32);
    let srand: unsafe extern "cdecl" fn(u32) = core::mem::transmute(rb(va::WA_SRAND));
    let rand_fn: unsafe extern "cdecl" fn() -> i32 = core::mem::transmute(rb(va::WA_RAND));
    srand(current_seed);
    let r1 = rand_fn() as u32;
    let r2 = rand_fn() as u32;
    wd(va::G_RANDOM_SEED, r2 + (r1 << 16));       // new seed
    wd(va::G_SAVED_RANDOM_SEED, current_seed);     // save old seed

    let ver = *(rb(va::G_REPLAY_VERSION_ID) as *const i32);
    if ver != 0x22 && !(ver >= 0x29 && ver <= 0x2A) && ver < 0x2D {
        let check: unsafe extern "cdecl" fn() -> i32 =
            core::mem::transmute(rb(va::SCHEME_CHECK_WEAPON_LIMITS));
        check();
    }
    // The map was already written to playback.thm in the header section.
    // The original loads it here via FUN_00447e80 + FUN_0044a9a0.
    // For positive sub-version (our test case), we need to:
    // Map loading: construct map object, load playback.thm, copy info, release.
    if (*gi).replay_map_type >= 1 {
        let alloc: unsafe extern "cdecl" fn(u32) -> *mut MapView =
            core::mem::transmute(rb(va::WA_CRT_MALLOC));
        let buf = alloc(core::mem::size_of::<MapView>() as u32);
        let map = if !buf.is_null() {
            let construct: unsafe extern "stdcall" fn(*mut MapView, i32) -> *mut MapView =
                core::mem::transmute(rb(va::MAP_VIEW_CONSTRUCTOR));
            construct(buf, 1)
        } else {
            ptr::null_mut()
        };

        let load: unsafe extern "stdcall" fn(*mut MapView, *const u8, i32) -> i32 =
            core::mem::transmute(rb(va::MAP_VIEW_LOAD));
        let ok = load(map, b"data\\playback.thm\0".as_ptr(), 0);

        if ok == 0 {
            if !map.is_null() {
                ((*(*map).vtable).destructor)(map, 1);
            }
            return Err(ReplayError::MapLoadFailure);
        }

        // Copy map info to game state (usercall ESI=map)
        call_usercall_esi(map as u32, rb(va::MAP_VIEW_COPY_INFO));

        // Terrain flag: zero = cavern terrain
        (*gi).terrain_flag = ((*map).terrain_flag == 0) as u8;

        // Release map object
        ((*(*map).vtable).destructor)(map, 1);
    }

    // Log output is written by the caller after map loading completes.
    Ok(())
}

// ─── Naked asm bridge for ProcessSchemeDefaults (usercall ESI=state) ─────────

// ─── Log output formatting ──────────────────────────────────────────────────



/// Write the /getlog replay header to the log file.
unsafe fn write_replay_log(gi: *const GameInfo, log_file: &mut File) -> Result<(), ReplayError> {
    use std::io::Write as IoWrite;
    let gi = &*gi;

    // ── Date/time line ───────────────────────────────────────────────────
    // "Game Started at YYYY-MM-DD HH:MM:SS GMT"
    // Only if DAT_0088C36C != 0 (recording timestamp available)
    // Date/time line: uses FUN_467780 (stdcall+usercall ESI=output)
    // and FUN_4677B0 + gmtime64. Complex calling convention.
    // For now, use the original's date formatting via a bridge call.
    // Date/time line: timestamp from observer array, formatted as GMT.
    // The observer array at 0x88C35C contains the replay date as a time_t
    // in its first registered entry. Use WA's gmtime64 for conversion.
    if *(rb(va::G_RECORDING_TIMESTAMP_FLAG) as *const u32) != 0 && REPLAY_TIMESTAMP != 0 {
        // Format the replay date using the timestamp saved during observer parsing.
        let ts = REPLAY_TIMESTAMP as i64;
        let gmtime: unsafe extern "cdecl" fn(*const i64) -> *const [i32; 9] =
            core::mem::transmute(rb(va::WA_GMTIME64));
        let tm = gmtime(&ts);
        if !tm.is_null() {
            let tm = &*tm;
            let mut s = heapless::String::<128>::new();
            let _ = write!(s,
                "Game Started at {:04}-{:02}-{:02} {:02}:{:02}:{:02} GMT\n",
                tm[5] + 1900, tm[4] + 1, tm[3], tm[2], tm[1], tm[0]
            );
            let _ = log_file.write_all(s.as_bytes());
        }
    }

    // ── Version lines ────────────────────────────────────────────────────
    // Line 1: "Game Engine Version: <version_string>"
    // Line 2: "File Format Version: <format_version>"

    let label_game_engine = wa_load_string(0x6E7);
    let label_file_format = wa_load_string(0x6E8); // "File Format Version"

    // File format version from version table
    let game_ver_id = *(rb(va::G_REPLAY_VERSION_ID) as *const i32);
    let version_str = if game_ver_id < 0 {
        match game_ver_id {
            -4 => rb(0x650EAC) as *const u8, // "1.0"
            -2 => rb(0x650EB0) as *const u8, // "3.0"
            -1 => rb(0x650EB4) as *const u8, // "3.5 Beta 1"
            _  => wa_load_string(0x0F),       // "Unknown"
        }
    } else {
        let table = rb(va::VERSION_STRING_TABLE) as *const u32;
        let ptr = *table.add(game_ver_id as usize);
        ptr as *const u8
    };

    let replay_ver = gi.replay_format_version;
    let format_ver_str = *(rb(0x6AC624 + replay_ver * 4) as *const u32) as *const u8;

    {
        let mut s = heapless::String::<512>::new();
        push_cstr(&mut s, label_game_engine);
        let _ = write!(s, ": ");
        push_cstr(&mut s, version_str);
        let _ = write!(s, "\n");
        push_cstr(&mut s, label_file_format);
        let _ = write!(s, ": ");
        push_cstr(&mut s, format_ver_str);
        let _ = write!(s, "\n");
        let _ = log_file.write_all(s.as_bytes());
    }

    // ── "Exported with Version" line ─────────────────────────────────────
    {
        let label_exported = wa_load_string(0x6E9); // "Exported with Version"
        // Current WA version from version table: 0x699814[DAT_00697702]
        let ver_byte = *(rb(va::G_VERSION_BYTE) as *const u8) as u32;
        // "3.8.1" literal at 0x641C60 + suffix from version table
        let ver_literal = rb(va::STR_VERSION_381) as *const u8; // "3.8.1"
        let ver_suffix = *(rb(va::VERSION_SUFFIX_TABLE + ver_byte * 4) as *const u32) as *const u8; // " (Steam)"

        let mut s = heapless::String::<256>::new();
        push_cstr(&mut s, label_exported);
        let _ = write!(s, ": ");
        push_cstr(&mut s, ver_literal);
        push_cstr(&mut s, ver_suffix);
        let _ = write!(s, "\n\n");
        let _ = log_file.write_all(s.as_bytes());
    }


    // ── Team listing ─────────────────────────────────────────────────────
    // For each team: "Color: "TeamName" [CPU X.XX]\n" or similar

    let color_names: [*const u8; 6] = [
        wa_load_string(0x20),
        wa_load_string(0x21),
        wa_load_string(0x22),
        wa_load_string(0x23),
        wa_load_string(0x24),
        wa_load_string(0x25),
    ];

    // Find max color name length for alignment — only active teams
    let teams = rb(va::G_TEAM_DATA) as *const ReplayTeamEntry;
    let mut max_color_len = 0usize;
    for slot in 0..6usize {
        let team = &*teams.add(slot);
        if team.flag == 0 { continue; }
        let ci = team.alliance as usize;
        if ci < 6 {
            let len = c_strlen(color_names[ci]);
            if len > max_color_len { max_color_len = len; }
        }
    }

    for slot in 0..6usize {
        let team = &*teams.add(slot);
        if team.flag == 0 { continue; }

        let team_type = team.team_type as i8;
        let color_idx = team.alliance as usize;

        let color = if color_idx < 6 { color_names[color_idx] } else { color_names[0] };
        let clen = c_strlen(color);

        // The displayed team name comes from the config_abbrev field
        // (e.g., "CPU 2"), not the custom team_name ("thrombosis1").
        let team_name = team.config_abbrev.as_ptr();

        let color_len = clen;

        let mut s = heapless::String::<256>::new();

        // Color name: capitalize first letter (original uses CharUpperA)
        if clen > 0 {
            let _ = s.push((*color as char).to_ascii_uppercase());
            push_cstr(&mut s, color.add(1));
        }
        let _ = write!(s, ":");
        for _ in 0..(max_color_len - color_len + 1) {
            let _ = s.push(' ');
        }

        // Quoted team name
        let _ = s.push('"');
        push_cstr(&mut s, team_name);
        let _ = s.push('"');

        // Team type info
        if (team_type as i32) < 0 {
            let abs_type = -(team_type as i32) as u32;
            let whole = abs_type / 20;
            let frac = (abs_type % 20) * 5;
            let cpu_label = wa_load_string(0x6ED);
            let _ = write!(s, " [");
            push_cstr(&mut s, cpu_label);
            let _ = write!(s, " {whole}.{frac:02}]");
        }

        let _ = write!(s, "\n");
        let _ = log_file.write_all(s.as_bytes());
    }

    let _ = log_file.write_all(b"\n");

    let _ = log_file.flush();
    Ok(())
}

/// Append a null-terminated C string to a heapless::String.
unsafe fn push_cstr<const N: usize>(s: &mut heapless::String<N>, cstr: *const u8) {
    let mut p = cstr;
    while *p != 0 {
        let _ = s.push(*p as char);
        p = p.add(1);
    }
}

/// Length of a null-terminated C string.
unsafe fn c_strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 { len += 1; }
    len
}

// ─── Naked asm bridges ──────────────────────────────────────────────────────

/// Bridge for usercall(EAX=value) + plain call. cdecl(eax_val, func_addr).
#[unsafe(naked)]
unsafe extern "cdecl" fn call_usercall_eax(_eax_val: *mut GameInfo, _func: u32) {
    core::arch::naked_asm!(
        "mov eax, [esp+4]",      // eax_val
        "mov ecx, [esp+8]",      // func_addr
        "call ecx",
        "ret",
    );
}

/// Bridge for usercall(ESI=value) + plain call. cdecl(esi_val, func_addr).
#[unsafe(naked)]
unsafe extern "cdecl" fn call_usercall_esi(_esi_val: u32, _func: u32) {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "mov esi, [esp+12]",     // esi_val
        "mov eax, [esp+16]",     // func_addr
        "call eax",
        "pop edi",
        "pop esi",
        "ret",
    );
}

/// Bridge for RegisterObserver: usercall(ESI=array) + stdcall(1 param=data_ptr).
/// cdecl(esi_val, data_ptr, func_addr).
#[unsafe(naked)]
unsafe extern "cdecl" fn call_register_observer(
    _esi_val: u32, _data_ptr: u32, _func: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "mov esi, [esp+12]",     // esi_val (0x88C35C rebased)
        "push [esp+16]",         // data_ptr → stdcall param
        "mov eax, [esp+24]",     // func_addr (shifted by push)
        "call eax",              // stdcall cleans 1 param (4 bytes)
        "pop edi",
        "pop esi",
        "ret",
    );
}

/// Bridge for ProcessSchemeDefaults: usercall(ESI=game_info).
/// Reuses call_usercall_esi.
#[inline]
unsafe fn call_process_scheme_defaults(gi: *mut GameInfo, func: u32) {
    call_usercall_esi(gi as u32, func);
}

// ─── ParseReplayPosition ─────────────────────────────────────────────────────

unsafe extern "stdcall" fn hook_parse_replay_position(input: *const u8) -> i32 {
    let mut len = 0usize;
    while *input.add(len) != 0 { len += 1; if len > 256 { break; } }
    let slice = core::slice::from_raw_parts(input, len + 1);
    replay::parse_replay_position(slice)
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        REPLAY_LOADER_ORIG = hook::install("ReplayLoader", va::REPLAY_LOADER, hook_replay_loader as *const ())? as *const ();
        PARSE_POSITION_ORIG = hook::install("ParseReplayPosition", va::PARSE_REPLAY_POSITION, hook_parse_replay_position as *const ())? as *const ();
    }
    Ok(())
}
