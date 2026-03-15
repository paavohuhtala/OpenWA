//! Speech / fanfare / FE SFX hooks.
//!
//! Hooks:
//! - PlayFeSfx (0x4D7960): stdcall(sfx_name), RET 0x4 — REPLACED
//! - PlayFanfare_Default (0x4D7500): stdcall(team_type), RET 0x4 — REPLACED
//! - PlayFanfare_CurrentTeam (0x4D78E0): usercall(EAX=index), plain RET — passthrough
//! - DSSound_LoadSpeechBank (0x571660): usercall(EAX=DDGameWrapper) + 3 stack, RET 0xC — REPLACED
//! - DSSound_LoadAllSpeechBanks (0x571A70): usercall(ESI=DDGameWrapper), plain RET — REPLACED

use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicU32, Ordering};

use heapless::CString;

use openwa_core::rebase::rb;
use openwa_core::engine::DDGameWrapper;
use openwa_core::address::va::{self, game_info_offsets};
use openwa_core::audio::SpeechLineTableEntry;

use crate::hook::{self, usercall_trampoline};
use crate::log_line;

/// Windows MAX_PATH (260 bytes including nul terminator).
const MAX_PATH: usize = 260;

// ============================================================
// WavPlayerHandle — typed wrapper for WA's WavPlayer instances
// ============================================================

/// Handle to a WA.exe WavPlayer instance (opaque, unknown size).
///
/// Known globals:
/// - `va::FESFX_WAV_PLAYER` (0x6AC888) — frontend sound effects
/// - `va::FANFARE_WAV_PLAYER` (0x6AC890) — team fanfares
#[derive(Clone, Copy)]
struct WavPlayerHandle(u32);

impl WavPlayerHandle {
    /// Read a WavPlayerHandle from a Ghidra global address.
    unsafe fn from_global(ghidra_addr: u32) -> Self {
        Self(rb(ghidra_addr))
    }

    /// Stop and release the current DirectSound buffer.
    /// WavPlayer_Stop: usercall(ESI=player, EDI=&result), plain RET.
    unsafe fn stop(self) {
        wav_player_stop_raw(self.0, rb(va::WAV_PLAYER_STOP));
    }

    /// Open a WAV file, parse RIFF, create DirectSound buffer.
    /// WavPlayer_LoadAndPlay: usercall(ESI=&result) + stack(player, path, 0), RET 0xC.
    unsafe fn load(self, path: *const c_char) {
        wav_player_load_raw(self.0, path.cast(), rb(va::WAV_PLAYER_LOAD_AND_PLAY));
    }

    /// Play the loaded buffer at the given volume.
    /// WavPlayer_Play: usercall(EDI=&result) + stack(player, volume), RET 0x8.
    unsafe fn play(self, volume: u32) {
        wav_player_play_raw(self.0, volume, rb(va::WAV_PLAYER_PLAY));
    }
}

// Naked asm wrappers for WavPlayer usercall functions.
// ESI/EDI are LLVM-reserved, so we must use naked functions.

/// cdecl(player, func_addr) — sets ESI=player, EDI=&result, calls func.
#[unsafe(naked)]
unsafe extern "cdecl" fn wav_player_stop_raw(_player: u32, _func: u32) {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "sub esp, 4",           // result on stack
        "mov esi, [esp + 16]",  // player (arg1, past saved esi+edi+result)
        "lea edi, [esp]",       // &result
        "call [esp + 20]",      // func (arg2)
        "add esp, 4",           // drop result
        "pop edi",
        "pop esi",
        "ret",
    );
}

/// cdecl(player, path, func_addr) — pushes stack params, sets ESI=&result, calls func.
/// Stack layout after push esi + sub esp,4:
///   ESP+0=result, +4=saved_esi, +8=retaddr, +12=player, +16=path, +20=func
/// Callee expects stack params: [ESP+4]=player, [ESP+8]=path, [ESP+12]=0
#[unsafe(naked)]
unsafe extern "cdecl" fn wav_player_load_raw(_player: u32, _path: *const u8, _func: u32) {
    core::arch::naked_asm!(
        "push esi",
        "sub esp, 4",              // result on stack
        "lea esi, [esp]",          // ESI = &result
        // Push in reverse order: 0, path, player
        "push 0",                  // third param=0 (ESP: +16=player, +20=path, +24=func)
        "push dword ptr [esp+20]", // second param=path (ESP: +20=player, +24=path, +28=func)
        "push dword ptr [esp+20]", // first param=player (ESP: +28=path, +32=func)
        "call dword ptr [esp+32]", // call func; RET 0xC cleans 3 params
        "add esp, 4",              // drop result
        "pop esi",
        "ret",
    );
}

/// cdecl(player, volume, func_addr) — pushes stack params, sets EDI=&result, calls func.
/// Stack layout after push edi + sub esp,4:
///   ESP+0=result, +4=saved_edi, +8=retaddr, +12=player, +16=volume, +20=func
/// Callee expects stack params: [ESP+4]=player, [ESP+8]=volume
#[unsafe(naked)]
unsafe extern "cdecl" fn wav_player_play_raw(_player: u32, _volume: u32, _func: u32) {
    core::arch::naked_asm!(
        "push edi",
        "sub esp, 4",              // result on stack
        "lea edi, [esp]",          // EDI = &result
        // Push in reverse order: volume, player
        "push dword ptr [esp+16]", // second param=volume (ESP: +16=player, +24=func)
        "push dword ptr [esp+16]", // first param=player (ESP: +28=func)
        "call dword ptr [esp+28]", // call func; RET 0x8 cleans 2 params
        "add esp, 4",              // drop result
        "pop edi",
        "ret",
    );
}


// ============================================================
// WA function wrappers
// ============================================================

/// GetTeamConfigName: usercall(ECX=index_0based, EAX=output_buf), plain RET.
/// Jump table with 49 cases (0-48). Writes null-terminated country/config
/// name (e.g. "Finland", "Simple", "USA") into the buffer at EAX.
unsafe fn get_team_config_name(index_0based: u32, buf: *mut u8) {
    core::arch::asm!(
        "call {func}",
        func = in(reg) rb(va::GET_TEAM_CONFIG_NAME),
        in("ecx") index_0based,
        in("eax") buf as u32,
        clobber_abi("C"),
    );
}

// ============================================================
// PlayFeSfx replacement (0x4D7960)
// ============================================================
// stdcall(sfx_name: *const u8), RET 0x4
// Builds "fesfx\<name>.wav", plays on FESFX_WAV_PLAYER.

unsafe extern "stdcall" fn hook_play_fe_sfx(sfx_name: *const u8) {
    let name = if !sfx_name.is_null() {
        CStr::from_ptr(sfx_name as *const i8)
            .to_str()
            .unwrap_or("?")
    } else {
        "(null)"
    };
    let _ = log_line(&format!("[Speech] PlayFeSfx: \"{}\"", name));

    // Build null-terminated path: "fesfx\<name>.wav"
    let mut path = CString::<MAX_PATH>::new();
    let _ = path.extend_from_bytes(b"fesfx\\");
    let _ = path.extend_from_bytes(name.as_bytes());
    let _ = path.extend_from_bytes(b".wav");

    let player = WavPlayerHandle::from_global(va::FESFX_WAV_PLAYER);
    player.stop();
    player.load(path.as_ptr());
    player.play(0);
}

// ============================================================
// PlayFanfare_Default replacement (0x4D7500)
// ============================================================
// stdcall(team_type: u32), RET 0x4
// If team_type in 1..=49, looks up config name; else "Simple".
// Builds "<wa_path>\user\Fanfare\<name>.wav", plays on FANFARE_WAV_PLAYER.

unsafe extern "stdcall" fn hook_play_fanfare_default(team_type: u32) {
    // Get the fanfare name
    let mut name_buf = [0u8; 64];
    if (1..=49).contains(&team_type) {
        get_team_config_name(team_type - 1, name_buf.as_mut_ptr());
    } else {
        name_buf[..7].copy_from_slice(b"Simple\0");
    }
    let name = CStr::from_ptr(name_buf.as_ptr() as *const i8)
        .to_str()
        .unwrap_or("Simple");

    // Read WA data path
    let wa_path = CStr::from_ptr(rb(va::WA_DATA_PATH) as *const i8)
        .to_str()
        .unwrap_or(".");

    let _ = log_line(&format!(
        "[Speech] PlayFanfare_Default: team_type={} name=\"{}\"",
        team_type, name
    ));

    // Build null-terminated path: "<wa_path>\user\Fanfare\<name>.wav"
    let mut path = CString::<MAX_PATH>::new();
    let _ = path.extend_from_bytes(wa_path.as_bytes());
    let _ = path.extend_from_bytes(b"\\user\\Fanfare\\");
    let _ = path.extend_from_bytes(name.as_bytes());
    let _ = path.extend_from_bytes(b".wav");

    let player = WavPlayerHandle::from_global(va::FANFARE_WAV_PLAYER);
    player.stop();
    player.load(path.as_ptr());
    player.play(0);
}

// ============================================================
// PlayFanfare_CurrentTeam passthrough (0x4D78E0)
// ============================================================
// usercall(EAX=index), plain RET, returns EAX = u32
// EAX is used as an implicit input (MOV EDI,EAX at 0x4D78E8).

static ORIG_PLAY_FANFARE_CURRENT_TEAM: AtomicU32 = AtomicU32::new(0);

usercall_trampoline!(fn trampoline_play_fanfare_current_team;
    impl_fn = play_fanfare_current_team_impl; reg = eax);

unsafe extern "cdecl" fn play_fanfare_current_team_impl(index: u32) -> u32 {
    let _ = log_line(&format!("[Speech] PlayFanfare_CurrentTeam: eax={}", index));

    let orig = ORIG_PLAY_FANFARE_CURRENT_TEAM.load(Ordering::Relaxed);
    let result: u32;
    core::arch::asm!(
        "call {orig}",
        orig = in(reg) orig,
        in("eax") index,
        lateout("eax") result,
        clobber_abi("C"),
    );

    let _ = log_line(&format!("[Speech] PlayFanfare_CurrentTeam => {}", result));
    result
}

// ============================================================
// DDGameWrapper__LoadSpeechWAV — pure Rust (replaces 0x571530)
// ============================================================
// Searches DDGameWrapper's speech name table for existing WAV, reuses slot if found.
// Otherwise calls DSSound vtable load_wav to load new WAV into a buffer.
// Stores the buffer slot index in the SpeechSlotTable.

/// Pure Rust replacement for DDGameWrapper__LoadSpeechWAV (0x571530).
///
/// Searches the name table for `wav_path`. If found, reuses the existing
/// buffer slot. Otherwise calls DSSound::load_wav to create a new buffer.
/// Returns 1 on success, 0 on failure.
unsafe fn call_load_speech_wav(
    ddgw: u32,
    team_index: u32,
    line_id: u32,
    wav_path: *const c_char,
    full_path: *const c_char,
) -> u32 {
    use openwa_core::audio::SpeechSlotTable;
    use openwa_core::engine::DDGameWrapper;
    use openwa_core::engine::ddgame_wrapper::SPEECH_NAME_ENTRY_SIZE;

    let wrapper = &mut *(ddgw as *mut DDGameWrapper);
    let count = wrapper.speech_name_count as usize;
    let search_name = core::ffi::CStr::from_ptr(wav_path).to_bytes();

    // Search name table for existing entry matching wav_path.
    let mut found_idx: Option<usize> = None;
    for i in 0..count {
        let entry = &wrapper.speech_name_table[i];
        // Compare up to the null terminator.
        let entry_len = entry.iter().position(|&b| b == 0).unwrap_or(entry.len());
        if entry[..entry_len] == *search_name {
            found_idx = Some(i);
            break;
        }
    }

    let ddgame = &mut *wrapper.ddgame;
    let slot_table = &mut ddgame.speech_slot_table;

    if let Some(idx) = found_idx {
        // Found existing — reuse the buffer slot.
        slot_table.set(team_index as usize, line_id, idx as u32 + SpeechSlotTable::BUFFER_OFFSET);
        return 1;
    }

    // Not found — load new WAV via DSSound vtable slot 12 (load_wav).
    let slot_idx = count as u32 + SpeechSlotTable::BUFFER_OFFSET;
    let vtable = (*wrapper.sound).vtable;
    let result = ((*vtable).load_wav)(wrapper.sound, slot_idx as i32, full_path as *const u8);

    if result != 0 {
        // Success — store slot, copy name into table, increment count.
        slot_table.set(team_index as usize, line_id, slot_idx);

        // Copy wav_path into name table entry.
        let dest = &mut wrapper.speech_name_table[count];
        let copy_len = search_name.len().min(SPEECH_NAME_ENTRY_SIZE - 1);
        dest[..copy_len].copy_from_slice(&search_name[..copy_len]);
        dest[copy_len] = 0;

        wrapper.speech_name_count = (count + 1) as u32;
        return 1;
    }

    0 // failure
}

// ============================================================
// Helpers
// ============================================================

/// Extract bytes from a null-terminated C string pointer.
unsafe fn cstr_bytes<'a>(ptr: *const u8) -> &'a [u8] {
    if ptr.is_null() {
        return &[];
    }
    CStr::from_ptr(ptr as *const i8).to_bytes()
}

// ============================================================
// DSSound_LoadSpeechBank replacement (0x571660)
// ============================================================
// usercall(EAX=DDGameWrapper) + 3 stack params (team_index, speech_base_path, speech_dir)
// RET 0xC
//
// Iterates the speech line table at 0x6AF770. For each entry, builds WAV paths
// and calls LoadSpeechWAV. Falls back to default speech dir on failure.
// Skips remaining filename alternates when one succeeds.

usercall_trampoline!(fn trampoline_load_speech_bank; impl_fn = load_speech_bank_impl;
    reg = eax; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn load_speech_bank_impl(
    ddgw: *const DDGameWrapper,
    team_index: u32,
    speech_base_path: *const u8,
    speech_dir: *const u8,
) {
    let path_str = CStr::from_ptr(speech_base_path as *const i8)
        .to_str()
        .unwrap_or("?");
    let dir_str = CStr::from_ptr(speech_dir as *const i8)
        .to_str()
        .unwrap_or("?");
    let _ = log_line(&format!(
        "[Speech] LoadSpeechBank: team={} path=\"{}\" dir=\"{}\" ddgw=0x{:08X}",
        team_index, path_str, dir_str, ddgw as u32
    ));

    let table_base = rb(va::SPEECH_LINE_TABLE) as *const SpeechLineTableEntry;
    let mut i: usize = 0;

    loop {
        let entry = &*table_base.add(i);

        // Null name_ptr = end of table
        if entry.name_ptr.is_null() {
            return;
        }

        let name_bytes = cstr_bytes(entry.name_ptr);

        // Build wav_path: "<speech_dir>\<name>.wav"
        let mut wav_path = CString::<MAX_PATH>::new();
        let _ = wav_path.extend_from_bytes(cstr_bytes(speech_dir));
        let _ = wav_path.extend_from_bytes(b"\\");
        let _ = wav_path.extend_from_bytes(name_bytes);
        let _ = wav_path.extend_from_bytes(b".wav");

        // Build full_path: "<speech_base_path>\<wav_path>"
        let mut full_path = CString::<MAX_PATH>::new();
        let _ = full_path.extend_from_bytes(cstr_bytes(speech_base_path));
        let _ = full_path.extend_from_bytes(b"\\");
        let _ = full_path.extend_from_bytes(wav_path.as_bytes());

        let result = call_load_speech_wav(
            ddgw as u32,
            team_index,
            entry.id,
            wav_path.as_ptr(),
            full_path.as_ptr(),
        );

        if result == 0 {
            // Failed — try fallback with default speech dir if no more alternates
            let next_entry = &*table_base.add(i + 1);
            if next_entry.name_ptr.is_null() || next_entry.id != entry.id {
                let game_info = (*(*ddgw).ddgame).game_info as *const u8;
                let default_dir =
                    game_info.add(game_info_offsets::DEFAULT_SPEECH_DIR as usize);
                let default_base =
                    game_info.add(game_info_offsets::DEFAULT_SPEECH_BASE_PATH as usize);

                let mut wav_path2 = CString::<MAX_PATH>::new();
                let _ = wav_path2.extend_from_bytes(cstr_bytes(default_dir));
                let _ = wav_path2.extend_from_bytes(b"\\");
                let _ = wav_path2.extend_from_bytes(name_bytes);
                let _ = wav_path2.extend_from_bytes(b".wav");

                let mut full_path2 = CString::<MAX_PATH>::new();
                let _ = full_path2.extend_from_bytes(cstr_bytes(default_base));
                let _ = full_path2.extend_from_bytes(b"\\");
                let _ = full_path2.extend_from_bytes(wav_path2.as_bytes());

                call_load_speech_wav(
                    ddgw as u32,
                    team_index,
                    entry.id,
                    wav_path2.as_ptr(),
                    full_path2.as_ptr(),
                );
            }
        } else {
            // Success — skip remaining filename alternates for this ID
            let current_id = entry.id;
            while {
                let next = &*table_base.add(i + 1);
                !next.name_ptr.is_null() && next.id == current_id
            } {
                i += 1;
            }
        }

        i += 1;
    }
}

// ============================================================
// DSSound_LoadAllSpeechBanks replacement (0x571A70)
// ============================================================
// usercall(ESI=DDGameWrapper), plain RET.
// Clears the speech slot table, then iterates teams and calls LoadSpeechBank.

usercall_trampoline!(fn trampoline_load_all_speech_banks;
    impl_fn = load_all_speech_banks_impl; reg = esi);

unsafe extern "cdecl" fn load_all_speech_banks_impl(ddgw: *const DDGameWrapper) {
    let ddgame = &mut *(*ddgw).ddgame;

    // Clear speech slot table
    ddgame.speech_slot_table.clear();

    // Read team count from GameInfo
    let game_info = &*ddgame.game_info;
    let team_count = game_info.speech_team_count as u32;

    let _ = log_line(&format!(
        "[Speech] LoadAllSpeechBanks: ddgw=0x{:08X} teams={}",
        ddgw as u32, team_count
    ));

    // Load speech bank for each team
    let gi = ddgame.game_info as *const u8;
    for i in 0..team_count {
        let team_offset = (i * game_info_offsets::SPEECH_TEAM_STRIDE) as usize;
        let base_path =
            gi.add(game_info_offsets::SPEECH_BASE_PATH as usize + team_offset);
        let dir =
            gi.add(game_info_offsets::SPEECH_DIR as usize + team_offset);

        // Call our Rust impl directly (bypasses the hook trampoline)
        load_speech_bank_impl(ddgw, i, base_path, dir);
    }
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "PlayFeSfx",
            va::PLAY_FE_SFX,
            hook_play_fe_sfx as *const (),
        )?;

        hook::install(
            "PlayFanfare_Default",
            va::PLAY_FANFARE_DEFAULT,
            hook_play_fanfare_default as *const (),
        )?;

        let trampoline = hook::install(
            "PlayFanfare_CurrentTeam",
            va::PLAY_FANFARE_CURRENT_TEAM,
            trampoline_play_fanfare_current_team as *const (),
        )?;
        ORIG_PLAY_FANFARE_CURRENT_TEAM.store(trampoline as u32, Ordering::Relaxed);

        // Full replacement — no trampoline needed
        hook::install(
            "DSSound_LoadSpeechBank",
            va::DSSOUND_LOAD_SPEECH_BANK,
            trampoline_load_speech_bank as *const (),
        )?;

        // Full replacement — no trampoline needed
        hook::install(
            "DSSound_LoadAllSpeechBanks",
            va::DSSOUND_LOAD_ALL_SPEECH_BANKS,
            trampoline_load_all_speech_banks as *const (),
        )?;
    }

    Ok(())
}
