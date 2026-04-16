//! Speech / fanfare / FE SFX / WavPlayer hooks.
//!
//! Thin hook shims — game logic lives in `openwa_core::audio::speech_ops`
//! and `openwa_core::audio::wav_player`.
//!
//! Hooks:
//! - WavPlayer_Stop (0x599670): usercall(ESI=player, EDI=&result), plain RET — REPLACED
//! - WavPlayer_Play (0x5996E0): usercall(EDI=&result) + stack(player, flags), RET 0x8 — REPLACED
//! - WavPlayer_LoadAndPlay (0x599B40): usercall(ESI=&result) + stack(player, path, param3), RET 0xC — REPLACED
//! - PlayFeSfx (0x4D7960): stdcall(sfx_name), RET 0x4 — REPLACED
//! - PlayFanfare_Default (0x4D7500): stdcall(team_type), RET 0x4 — REPLACED
//! - PlayFanfare_CurrentTeam (0x4D78E0): usercall(EAX=index), plain RET — REPLACED
//! - DSSound_LoadSpeechBank (0x571660): usercall(EAX=DDGameWrapper) + 3 stack, RET 0xC — REPLACED
//! - DSSound_LoadAllSpeechBanks (0x571A70): usercall(ESI=DDGameWrapper), plain RET — REPLACED

use std::ffi::{CStr, c_char};

use openwa_core::address::va;
use openwa_core::audio::wav_player::WavPlayer;
use openwa_core::audio::{speech_ops, wav_player};
use openwa_core::engine::DDGameWrapper;
use openwa_core::rebase::rb;

use crate::hook::{self, usercall_trampoline};
use crate::log_line;

// ============================================================
// WavPlayer hook trampolines (usercall → cdecl shims)
// ============================================================

// WavPlayer_Stop: usercall(ESI=player, EDI=&result), plain RET.
#[unsafe(naked)]
unsafe extern "C" fn hook_wav_player_stop() {
    core::arch::naked_asm!(
        "push esi",
        "call {impl_fn}",
        "add esp, 4",
        "mov eax, [{success_val}]",
        "mov [edi], eax",
        "mov eax, edi",
        "ret",
        impl_fn = sym wav_player_stop_impl,
        success_val = sym WAV_RESULT_SUCCESS_VALUE,
    );
}

unsafe extern "cdecl" fn wav_player_stop_impl(player: *mut WavPlayer) {
    unsafe {
        wav_player::wav_player_stop(player);
    }
}

// WavPlayer_Play: usercall(EDI=&result) + stack(player, flags), RET 0x8.
#[unsafe(naked)]
unsafe extern "C" fn hook_wav_player_play() {
    core::arch::naked_asm!(
        "push dword ptr [esp+8]",
        "push dword ptr [esp+8]",
        "call {impl_fn}",
        "add esp, 8",
        "mov eax, [{success_val}]",
        "mov [edi], eax",
        "mov eax, edi",
        "ret 0x8",
        impl_fn = sym wav_player_play_impl,
        success_val = sym WAV_RESULT_SUCCESS_VALUE,
    );
}

unsafe extern "cdecl" fn wav_player_play_impl(player: *mut WavPlayer, flags: u32) {
    unsafe {
        wav_player::wav_player_play(player, flags);
    }
}

// WavPlayer_LoadAndPlay: usercall(ESI=&result) + stack(player, path, param3), RET 0xC.
#[unsafe(naked)]
unsafe extern "C" fn hook_wav_player_load_and_play() {
    core::arch::naked_asm!(
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "call {impl_fn}",
        "add esp, 12",
        "test eax, eax",
        "jz 2f",
        "mov eax, [{success_val}]",
        "mov [esi], eax",
        "mov eax, esi",
        "ret 0xC",
        "2:",
        "mov dword ptr [esi], 0xFFFFFFFF",
        "mov eax, esi",
        "ret 0xC",
        impl_fn = sym wav_player_load_and_play_impl,
        success_val = sym WAV_RESULT_SUCCESS_VALUE,
    );
}

unsafe extern "cdecl" fn wav_player_load_and_play_impl(
    player: *mut WavPlayer,
    path: *const c_char,
    param3: i32,
) -> u32 {
    unsafe { wav_player::wav_player_load_and_play(player, path, param3) as u32 }
}

/// The success sentinel VALUE (read from *(0x8AC8A0+delta) at install time).
static mut WAV_RESULT_SUCCESS_VALUE: u32 = 0;

// ============================================================
// PlayFeSfx hook (0x4D7960)
// ============================================================

unsafe extern "stdcall" fn hook_play_fe_sfx(sfx_name: *const u8) {
    unsafe {
        let name = if !sfx_name.is_null() {
            CStr::from_ptr(sfx_name as *const i8)
                .to_str()
                .unwrap_or("?")
        } else {
            "(null)"
        };
        let _ = log_line(&format!("[Speech] PlayFeSfx: \"{}\"", name));
        speech_ops::play_fe_sfx(name);
    }
}

// ============================================================
// PlayFanfare_Default hook (0x4D7500)
// ============================================================

unsafe extern "stdcall" fn hook_play_fanfare_default(team_type: u32) {
    unsafe {
        let _ = log_line(&format!(
            "[Speech] PlayFanfare_Default: team_type={}",
            team_type
        ));
        speech_ops::play_fanfare_default(team_type);
    }
}

// ============================================================
// PlayFanfare_CurrentTeam hook (0x4D78E0)
// ============================================================

usercall_trampoline!(fn trampoline_play_fanfare_current_team;
    impl_fn = play_fanfare_current_team_impl; reg = eax);

unsafe extern "cdecl" fn play_fanfare_current_team_impl(eax_index: u32) -> u32 {
    unsafe {
        let _ = log_line(&format!(
            "[Speech] PlayFanfare_CurrentTeam: eax={}",
            eax_index
        ));
        speech_ops::play_fanfare_current_team(eax_index)
    }
}

// ============================================================
// DSSound_LoadSpeechBank hook (0x571660)
// ============================================================

usercall_trampoline!(fn trampoline_load_speech_bank; impl_fn = load_speech_bank_impl;
    reg = eax; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn load_speech_bank_impl(
    ddgw: *const DDGameWrapper,
    team_index: u32,
    speech_base_path: *const u8,
    speech_dir: *const u8,
) {
    unsafe {
        let path_str = CStr::from_ptr(speech_base_path as *const i8)
            .to_str()
            .unwrap_or("?");
        let dir_str = CStr::from_ptr(speech_dir as *const i8)
            .to_str()
            .unwrap_or("?");
        let _ = log_line(&format!(
            "[Speech] LoadSpeechBank: team={} path=\"{}\" dir=\"{}\"",
            team_index, path_str, dir_str
        ));
        speech_ops::load_speech_bank(ddgw, team_index, speech_base_path, speech_dir);
    }
}

// ============================================================
// DSSound_LoadAllSpeechBanks hook (0x571A70)
// ============================================================

usercall_trampoline!(fn trampoline_load_all_speech_banks;
    impl_fn = load_all_speech_banks_impl; reg = esi);

unsafe extern "cdecl" fn load_all_speech_banks_impl(ddgw: *const DDGameWrapper) {
    unsafe {
        let _ = log_line(&format!(
            "[Speech] LoadAllSpeechBanks: ddgw=0x{:08X}",
            ddgw as u32
        ));
        speech_ops::load_all_speech_banks(ddgw);
    }
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        // Read the success sentinel VALUE for hook trampolines.
        WAV_RESULT_SUCCESS_VALUE = *(rb(wav_player::G_WAV_RESULT_SUCCESS) as *const u32);

        // Global WavPlayer hooks — all callers (our hooks + unhooked WA code)
        // go through Rust. Avoids CRT heap mismatch.
        hook::install(
            "WavPlayer_Stop",
            va::WAV_PLAYER_STOP,
            hook_wav_player_stop as *const (),
        )?;
        hook::install(
            "WavPlayer_Play",
            va::WAV_PLAYER_PLAY,
            hook_wav_player_play as *const (),
        )?;
        hook::install(
            "WavPlayer_LoadAndPlay",
            va::WAV_PLAYER_LOAD_AND_PLAY,
            hook_wav_player_load_and_play as *const (),
        )?;

        hook::install("PlayFeSfx", va::PLAY_FE_SFX, hook_play_fe_sfx as *const ())?;
        hook::install(
            "PlayFanfare_Default",
            va::PLAY_FANFARE_DEFAULT,
            hook_play_fanfare_default as *const (),
        )?;
        hook::install(
            "PlayFanfare_CurrentTeam",
            va::PLAY_FANFARE_CURRENT_TEAM,
            trampoline_play_fanfare_current_team as *const (),
        )?;
        hook::install(
            "DSSound_LoadSpeechBank",
            va::DSSOUND_LOAD_SPEECH_BANK,
            trampoline_load_speech_bank as *const (),
        )?;
        hook::install(
            "DSSound_LoadAllSpeechBanks",
            va::DSSOUND_LOAD_ALL_SPEECH_BANKS,
            trampoline_load_all_speech_banks as *const (),
        )?;
    }

    Ok(())
}
