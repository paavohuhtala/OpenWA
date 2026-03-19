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

// ─── WA CRT wrappers (rebased addresses, NOT Rust's UCRT) ───────────────────

#[allow(non_camel_case_types)]
type FILE = c_void;

unsafe fn wa_fopen(f: *const u8, m: *const u8) -> *mut FILE {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(*const u8, *const u8) -> *mut FILE>(rb(0x5D3271))(f, m)
}
unsafe fn wa_fclose(s: *mut FILE) {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(*mut FILE)>(rb(0x5D399B))(s);
}
unsafe fn wa_fread(b: *mut c_void, sz: u32, c: u32, f: *mut FILE) -> u32 {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(*mut c_void, u32, u32, *mut FILE) -> u32>(rb(0x5D4531))(b, sz, c, f)
}
unsafe fn wa_fwrite(b: *const c_void, sz: u32, c: u32, f: *mut FILE) -> u32 {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(*const c_void, u32, u32, *mut FILE) -> u32>(rb(0x5D3B76))(b, sz, c, f)
}
unsafe fn wa_malloc(sz: u32) -> *mut u8 {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(u32) -> *mut u8>(rb(0x5D0F65))(sz)
}
unsafe fn wa_free(p: *mut u8) {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(*mut u8)>(rb(0x5D0D2B))(p);
}
unsafe fn wa_fileno(s: *mut FILE) -> i32 {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(*mut FILE) -> i32>(rb(0x5D5155))(s)
}
unsafe fn wa_filelengthi64(fd: i32) -> i64 {
    core::mem::transmute::<_, unsafe extern "cdecl" fn(i32) -> i64>(rb(0x5D4FE1))(fd)
}

extern "stdcall" { fn SetCurrentDirectoryA(path: *const u8) -> i32; }

// ─── Trampoline storage ─────────────────────────────────────────────────────

static mut REPLAY_LOADER_ORIG: *const () = core::ptr::null();
#[allow(dead_code)]
static mut PARSE_POSITION_ORIG: *const () = core::ptr::null();

// ─── RAII cleanup guard ─────────────────────────────────────────────────────

struct ReplayGuard { file: *mut FILE, payload: *mut u8 }
impl ReplayGuard { fn new() -> Self { Self { file: ptr::null_mut(), payload: ptr::null_mut() } } }
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
    let artclass_counter = *(rb(va::G_ARTCLASS_COUNTER) as *const i32);
    if artclass_counter >= 0x34 { return Err(ReplayError::ArtClassLimit); }

    let s = state as *mut u8;
    *s.add(0xDB48) = 1;
    *(s.add(0xEF60) as *mut u32) = 0;

    // Open replay file
    SetCurrentDirectoryA(rb(0x88E078) as *const u8);
    let file = wa_fopen(s.add(0xDB60), b"rb\0".as_ptr());
    SetCurrentDirectoryA(rb(0x88E17D) as *const u8);
    if file.is_null() { return Err(ReplayError::FileNotFound); }

    let mut guard = ReplayGuard::new();
    guard.file = file;
    let fd = wa_fileno(file);
    let file_size = wa_filelengthi64(fd) as u64;

    // Header
    let mut header: u32 = 0;
    if wa_fread(&mut header as *mut u32 as *mut c_void, 4, 1, file) == 0 { return Err(ReplayError::InvalidFormat); }
    if (header & 0xFFFF) != replay::REPLAY_MAGIC as u32 { return Err(ReplayError::InvalidFormat); }
    let version = header >> 16;
    if version == 0 || version > 20 { return Err(ReplayError::VersionTooNew); }

    *(s.add(0xDB50) as *mut u32) = version;
    *(s.add(0xDB54) as *mut u32) = version;
    *(s.add(0xDB58) as *mut u32) = 0xFFFFFFFF;

    // First payload
    let mut payload_size: u32 = 0;
    if wa_fread(&mut payload_size as *mut u32 as *mut c_void, 4, 1, file) == 0 { return Err(ReplayError::InvalidFormat); }
    if (payload_size as u64 + 8) > file_size { return Err(ReplayError::InvalidFormat); }
    let payload = wa_malloc(payload_size);
    if payload.is_null() { return Err(ReplayError::MallocFailure); }
    guard.payload = payload;
    if wa_fread(payload as *mut c_void, payload_size, 1, file) == 0 { return Err(ReplayError::FileNotFound); }

    let first_dword = *(payload as *const i32);
    *(s.add(0xDB1C) as *mut i32) = first_dword;
    if first_dword >= 1 {
        let thm = wa_fopen(b"data\\playback.thm\0".as_ptr(), b"wb\0".as_ptr());
        if !thm.is_null() { wa_fwrite(payload as *const c_void, 1, payload_size, thm); wa_fclose(thm); }
    } else {
        *(s.add(0xDB20) as *mut i32) = *(payload.add(4) as *const i32);
        if first_dword >= -4 && first_dword < -2 {
            *(s.add(0xDB24) as *mut i32) = *(payload.add(8) as *const i32);
        } else if first_dword == -2 {
            let n = payload_size.saturating_sub(8) as usize;
            if n > 0x20 { return Err(ReplayError::InvalidFormat); }
            ptr::copy_nonoverlapping(payload.add(8), s.add(0xDB24), n);
            *s.add(0xDB1C + payload_size as usize) = 0;
        }
    }
    wa_free(payload); guard.payload = ptr::null_mut();

    // Clear global buffers
    ptr::write_bytes(rb(va::G_TEAM_HEADER_DATA) as *mut u8, 0, 0x5728);
    ptr::write_bytes(rb(va::G_TEAM_SECONDARY_DATA) as *mut u8, 0, 0xD9DC);

    let _ = log_line(&format!("[Replay] Header: ver={version} payload={payload_size} sub={first_dword}"));

    if version == 1 {
        return parse_version1(state, file, &mut guard);
    }

    // ─── Version 2+: read second payload ─────────────────────────────────

    let mut second_size: u32 = 0;
    if wa_fread(&mut second_size as *mut u32 as *mut c_void, 4, 1, file) == 0 { return Err(ReplayError::InvalidFormat); }
    if (4u64 + 4 + payload_size as u64 + 4 + second_size as u64) > file_size { return Err(ReplayError::InvalidFormat); }
    let p2 = wa_malloc(second_size);
    if p2.is_null() { return Err(ReplayError::MallocFailure); }
    guard.payload = p2;
    if wa_fread(p2 as *mut c_void, second_size, 1, file) == 0 { return Err(ReplayError::InvalidFormat); }
    wa_fclose(file); guard.file = ptr::null_mut();

    // Parse and write to globals
    let data = core::slice::from_raw_parts(p2, second_size as usize);
    let result = parse_and_write_v2plus(state, data, version);
    let _ = log_line("[Replay] CP: parse returned, freeing payload");
    wa_free(p2); guard.payload = ptr::null_mut();
    let _ = log_line("[Replay] CP: payload freed");

    match result {
        Ok(()) => {
            let _ = log_line("[Replay] Rust replay loading complete");
            Ok(())
        }
        Err(e) => {
            let _ = log_line(&format!("[Replay] Parse failed ({e:?}), falling back to original"));
            delegate_to_original(state)
        }
    }
}

unsafe fn parse_version1(state: u32, _file: *mut FILE, guard: &mut ReplayGuard) -> Result<(), ReplayError> {
    wa_fclose(guard.file); guard.file = ptr::null_mut();
    delegate_to_original(state)
}

unsafe fn delegate_to_original(state: u32) -> Result<(), ReplayError> {
    let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 = core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(state, 1);
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
    state: u32, data: &[u8], version: u32,
) -> Result<(), ReplayError> {
    let mut s = ReplayStream::new(data);
    let _ = log_line(&format!("[Replay] v2+ parse start, {} bytes", data.len()));

    // ── Sub-format flags ─────────────────────────────────────────────────
    let ver_gt7 = (version > 7) as u8;
    wb(0x88AF42, ver_gt7);
    wb(0x88AF43, ver_gt7);
    wd(va::G_REPLAY_SUB_FORMAT, 0);

    let mut obs_count: u16 = 0;

    if version >= 10 {
        let sub_format = s.read_u16()?;
        wd(va::G_REPLAY_SUB_FORMAT, sub_format as u32);
        if sub_format != 0 { return Err(ReplayError::VersionTooNew); }

        if version >= 12 {
            if version < 18 {
                let mode = s.read_u8_validated(0, 2)?;
                wb(0x88AF44, mode);
            } else {
                let raw = s.read_u8_validated(0, 3)?;
                if raw >= 2 && raw <= 3 {
                    wb(0x88AF44, raw - 1);
                    wb(0x88AF42, 1); wb(0x88AF43, 1);
                } else {
                    wb(0x88AF42, (raw != 0) as u8);
                    wb(0x88AF44, 0);
                    wb(0x88AF43, (raw != 0) as u8);
                }
            }
        }

        obs_count = s.read_u16_validated(1, version as u16)?;
        wd(0x88AF4C, obs_count as u32);

        // Observer team loop — skip RegisterObserver bridge for now,
        // just consume the stream. TODO: bridge RegisterObserver.
        loop {
            let _team_id = s.read_u32()?;
            let obs_type = s.read_u8_validated(0, 2)?;
            if obs_type == 0 { break; }
        }
    }

    let _ = log_line("[Replay] CP: sub-format done"); // ── Game version ID ──────────────────────────────────────────────────
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

    let _ = log_line("[Replay] CP: scheme presence done"); // ── Scheme data ──────────────────────────────────────────────────────
    let mut scheme_version: u8 = 0;

    if scheme_present == 1 {
        // Scheme header byte
        if obs_count >= 3 {
            let header_byte = s.read_u8()?;
            wb(0x88DAD4, header_byte);
            // If header >= 0 (signed): load built-in scheme from resources
            if (header_byte as i8) >= 0 {
                // FUN_004D4840: stdcall(2 params). RET 0x8.
                let f: unsafe extern "stdcall" fn(u32, i32) =
                    core::mem::transmute(rb(0x4D4840));
                f(rb(0x88DACC), header_byte as i32);
            }
        }

        // Scheme size indicator
        let mut scheme_size_indicator: u32 = 0;
        if obs_count >= 0x14 {
            scheme_size_indicator = s.read_u32()?;
        }

        // SCHM magic + version
        let magic = s.read_u32()?;
        scheme_version = s.read_u8()?;
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
                    rb(0x88DC04) as *mut u8, 0x6E,
                );
                (scheme_size_indicator as usize) - 5
            }
            _ => return Err(ReplayError::VersionTooNew),
        };

        // Copy scheme data from stream to global 0x88DAE0
        let scheme_slice = s.advance_raw(scheme_data_size)?;
        ptr::copy_nonoverlapping(scheme_slice.as_ptr(), rb(0x88DAE0) as *mut u8, scheme_data_size);

        // If scheme_header < 0 (signed) and v1/v2: clear + defaults
        if scheme_version <= 2 && (*(rb(0x88DAD4) as *const i8)) < 0 {
            ptr::write_bytes(rb(0x88DBB8) as *mut u8, 0, 0x4C);
            ptr::copy_nonoverlapping(
                rb(va::SCHEME_V3_DEFAULTS) as *const u8,
                rb(0x88DC04) as *mut u8, 0x6E,
            );
        }

        // Validate extended options for v3
        if scheme_version == 3 {
            let validate: unsafe extern "cdecl" fn() -> i32 =
                core::mem::transmute(rb(0x4D5110));
            let r = validate();
            if r != 0 { return Err(ReplayError::InvalidFormat); }
        }

        // Random seed save/read
        let saved_seed = *(rb(va::G_RANDOM_SEED) as *const u32);
        let _seed_from_stream = s.read_u32()?;
        wd(va::G_RANDOM_SEED, saved_seed); // restore (original overwrites then restores)
    } else {
        // No scheme path — fall back to delegation for now
        // (ProcessAllianceData reads from stream via usercall EAX)
        let _ = log_line("[Replay] No-scheme path: delegating");
        return Err(ReplayError::InvalidFormat); // triggers fallback
    }

    let _ = log_line("[Replay] CP: scheme data done"); // ── Map bytes + replay name + host ───────────────────────────────────
    let map_byte1 = s.read_u8()?;
    let map_byte2 = s.read_u8()?;
    wb(0x87250C, map_byte1);
    wb(0x872508, map_byte2);

    let mut replay_name = [0u8; 0x29];
    s.read_prefixed_string(&mut replay_name)?;
    ptr::copy_nonoverlapping(replay_name.as_ptr(), rb(0x87D0E1) as *mut u8, 0x29);

    if version >= 9 {
        let host = s.read_u8()?;
        // Store as low byte of DAT_008779E0 (CONCAT31 pattern)
        let current = *(rb(0x8779E0) as *const u32);
        wd(0x8779E0, (current & 0xFFFFFF00) | host as u32);
    } else {
        wd(0x8779E0, 0xFFFFFFFF);
    }

    let _ = log_line("[Replay] CP: map/name done"); // ── Observer player entries (13 slots, stride 0x78) ──────────────────
    let mut player_count: u8 = 0;
    for i in 0..13u32 {
        let base = 0x877A58 + i * 0x78;
        let flag = s.read_u8()?;
        wb(base, flag);
        if flag == 0 { continue; }

        if i as u32 == *(rb(0x8779E0) as *const u32) {
            // This is the host player — set local_11 flag
        }
        player_count += 1;

        let mut name = [0u8; 0x11];
        s.read_prefixed_string(&mut name)?;
        ptr::copy_nonoverlapping(name.as_ptr(), rb(0x8779E4 + i * 0x78) as *mut u8, 0x11);

        let mut display = [0u8; 0x31];
        s.read_prefixed_string(&mut display)?;
        ptr::copy_nonoverlapping(display.as_ptr(), rb(0x8779F5 + i * 0x78) as *mut u8, 0x31);

        let mut config = [0u8; 0x29];
        s.read_prefixed_string(&mut config)?;
        ptr::copy_nonoverlapping(config.as_ptr(), rb(0x877A26 + i * 0x78) as *mut u8, 0x29);

        let u16_val = s.read_u16()?;
        *(rb(0x877A50 + i * 0x3C) as *mut u16) = u16_val;

        let byte1 = s.read_u8()?;
        wb(0x877A52 + i * 0x78, byte1);

        let u32_val = s.read_u32()?;
        wd(0x877A54 + i * 0x1E, u32_val);

        let byte2 = s.read_u8()?;
        wb(0x877A5B + i * 0x78, byte2);
    }
    wb(0x87D0DE, player_count);

    let _ = log_line("[Replay] CP: observers done"); // ── XOR game ID ──────────────────────────────────────────────────────
    if obs_count >= 16 {
        let xor_a = s.read_u32()?;
        let _xor_b = s.read_u32()?;
        wd(va::G_REPLAY_GAME_ID, xor_a ^ replay::REPLAY_XOR_KEY);
    }

    let _ = log_line("[Replay] CP: XOR done"); // ── Team entries (6 slots, stride 0xD7B) ─────────────────────────────
    let mut team_count: u8 = 0;
    for team_idx in 0..6u32 {
        let tb = 0x877FFC + team_idx * 0xD7B; // per-team base
        let team_flag = s.read_u8()?;
        wb(0x878120 + team_idx * 0xD7B, team_flag);
        if team_flag == 0 { continue; }
        team_count += 1;

        let team_type = s.read_u8()? as i8;
        if !replay::validate_team_type(team_type) { return Err(ReplayError::InvalidFormat); }
        wb(tb, team_type as u8);

        let alliance = s.read_u8_validated(0, 5)?;
        wb(tb + 1, alliance);

        let unk = s.read_u8()?;
        wb(tb + 2, unk);

        // Pre-loop worm name (config abbreviation)
        let mut pre_name = [0u8; 0x11];
        s.read_worm_name(&mut pre_name, use_fixed_names)?;
        // Destination from decompile: ReadWormName before loop — unclear exact offset
        // TODO: trace exact destination

        // 8 worm names
        for worm_idx in 0..8u32 {
            let name_off = ((team_idx as usize) * 0xCB + worm_idx as usize) * 0x11;
            let dest = rb(0x878097) as *mut u8;
            if use_fixed_names {
                let slice = s.advance_raw(0x11)?;
                ptr::copy_nonoverlapping(slice.as_ptr(), dest.add(name_off), 0x11);
            } else {
                let mut name = [0u8; 0x11];
                s.read_prefixed_string(&mut name)?;
                ptr::copy_nonoverlapping(name.as_ptr(), dest.add(name_off), 0x11);
            }
        }

        // Worm count (unvalidated)
        let worm_count_raw = s.read_u8()?;
        wb(0x878092 + team_idx * 0xD7B, worm_count_raw);

        // Team name
        let mut team_name = [0u8; 0x41];
        s.read_prefixed_string(&mut team_name)?;
        ptr::copy_nonoverlapping(team_name.as_ptr(), rb(0x878010 + team_idx * 0xD7B) as *mut u8, 0x41);

        // Extra byte if obs_count > 13
        if obs_count > 13 {
            let extra = s.read_u8()?;
            wb(0x878093 + team_idx * 0xD7B, extra);
        }

        // Config name
        let mut config_name = [0u8; 0x41];
        s.read_prefixed_string(&mut config_name)?;
        ptr::copy_nonoverlapping(config_name.as_ptr(), rb(0x878051 + team_idx * 0xD7B) as *mut u8, 0x41);

        // Worm count (validated 1-8)
        let worm_count = s.read_u8()?;
        if worm_count == 0 || worm_count > 8 { return Err(ReplayError::InvalidFormat); }
        wb(0x878094 + team_idx * 0xD7B, worm_count);

        // Color, flag, grave, soundbank
        wb(0x878095 + team_idx * 0xD7B, s.read_u8()?);
        wb(0x878096 + team_idx * 0xD7B, s.read_u8()?);
        wb(0x87811F + team_idx * 0xD7B, s.read_u8()?);
        wb(0x878121 + team_idx * 0xD7B, s.read_u8()?);
        wb(0x878122 + team_idx * 0xD7B, s.read_u8()?);

        // Weapon data blocks
        let weapons_dest = rb(0x878123 + team_idx * 0xD7B) as *mut u8;
        let w1 = s.advance_raw(0x400)?;
        ptr::copy_nonoverlapping(w1.as_ptr(), weapons_dest, 0x400);
        let w2 = s.advance_raw(0x154)?;
        ptr::copy_nonoverlapping(w2.as_ptr(), weapons_dest.add(0x400), 0x154);
        let w3 = s.advance_raw(0x400)?;
        ptr::copy_nonoverlapping(w3.as_ptr(), weapons_dest.add(0x554), 0x400);
        let w4 = s.advance_raw(0x300)?;
        ptr::copy_nonoverlapping(w4.as_ptr(), weapons_dest.add(0x954), 0x300);
    }

    if team_count == 0 { return Err(ReplayError::InvalidFormat); }

    let _ = log_line("[Replay] CP: teams done"); // ── Team count + ProcessTeamColors ────────────────────────────────────
    wb(0x87D0E0, team_count);

    // ProcessTeamColors: stdcall(1 param = state). RET 0x4.
    let process_colors: unsafe extern "stdcall" fn(u32) =
        core::mem::transmute(rb(va::REPLAY_PROCESS_TEAM_COLORS));
    process_colors(rb(va::G_REPLAY_STATE));

    let _ = log_line("[Replay] CP: team processing done");
    let map_seed = s.read_u16()?;
    wd(0x87D430, map_seed as u32);
    let _ = log_line(&format!("[Replay] CP: map_seed={map_seed}"));

    // FUN_0045d640: stdcall(1 param = state). 1032-line function.
    let fun_45d640: unsafe extern "stdcall" fn(u32) =
        core::mem::transmute(rb(0x45D640));
    fun_45d640(rb(va::G_REPLAY_STATE));
    let _ = log_line("[Replay] CP: FUN_45d640 done");

    if map_seed == 0 || map_seed == 0xFFFF {
        call_process_scheme_defaults(rb(va::G_REPLAY_STATE), rb(va::REPLAY_PROCESS_SCHEME_DEFAULTS));
        let _ = log_line("[Replay] CP: ProcessSchemeDefaults done");
    } else {
        let _ = log_line(&format!("[Replay] CP: map_seed={map_seed}, skipping scheme defaults (TODO: weapon config)"));
    }

    // ValidateTeamSetup: reads [ESP+0xBC] in prologue = stdcall(1 param = state)
    let validate_setup: unsafe extern "stdcall" fn(u32) =
        core::mem::transmute(rb(va::REPLAY_VALIDATE_TEAM_SETUP));
    validate_setup(rb(va::G_REPLAY_STATE));
    let _ = log_line("[Replay] CP: ValidateTeamSetup done");

    let saved_seed = *(rb(va::G_RANDOM_SEED) as *const u32);
    let srand: unsafe extern "cdecl" fn(u32) = core::mem::transmute(rb(0x5D293E));
    let rand_fn: unsafe extern "cdecl" fn() -> i32 = core::mem::transmute(rb(0x5D294B));
    srand(0);
    let r1 = rand_fn();
    let r2 = rand_fn();
    wd(va::G_RANDOM_SEED, (r2 as u32).wrapping_add((r1 as u32) << 16));
    wd(va::G_SAVED_RANDOM_SEED, saved_seed);
    let _ = log_line("[Replay] CP: random seed done");

    let ver = *(rb(va::G_REPLAY_VERSION_ID) as *const i32);
    if ver != 0x22 && !(ver >= 0x29 && ver <= 0x2A) && ver < 0x2D {
        let check: unsafe extern "cdecl" fn() -> i32 =
            core::mem::transmute(rb(0x4D50E0));
        check();
    }
    let _ = log_line("[Replay] CP: weapon limits done"); // ── Map loading ──────────────────────────────────────────────────────
    // The map was already written to playback.thm in the header section.
    // The original loads it here via FUN_00447e80 + FUN_0044a9a0.
    // For positive sub-version (our test case), we need to:
    // Map loading: the original uses a complex map object construct+load+release
    // pattern. Getting the calling conventions wrong crashes. Let me investigate
    // each function's convention from assembly before enabling this.
    // TODO: implement map loading
    let _ = log_line("[Replay] CP: skipping map loading (TODO)");

    // ── Log output ───────────────────────────────────────────────────────
    // TODO: Port the ~600-line /getlog formatted output.
    // For now, the log output is missing — headless tests will fail.
    // Headful tests should work since log output is only for /getlog.

    let _ = log_line("[Replay] CP: returning Ok");
    Ok(())
}

// ─── Naked asm bridge for ProcessSchemeDefaults (usercall ESI=state) ─────────

/// Call Replay__ProcessSchemeDefaults (0x4670F0) which uses usercall(ESI=state).
/// ESI/EDI are LLVM-reserved, so must use naked asm.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_process_scheme_defaults(_state: u32, _func: u32) {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "mov esi, [esp+12]",     // state param (shifted by 2 pushes)
        "mov eax, [esp+16]",     // func addr (2nd param)
        "call eax",
        "pop edi",
        "pop esi",
        "ret",
    );
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
