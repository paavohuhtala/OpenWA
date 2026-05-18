//! Speech / fanfare / FE SFX / WavPlayer hooks.
//!
//! Thin hook shims — game logic lives in `openwa_game::audio::speech_ops`
//! and `openwa_game::audio::wav_player`.
//!
//! All hooks are codegen-driven via `hooks/speech.toml` + `re/**/*.toml`.

use std::ffi::{CStr, c_char, c_void};

use openwa_game::audio::speech_ops;
use openwa_game::audio::wav_player::{self, WavPlayer};
use openwa_game::engine::GameRuntime;

use crate::generated::hooks;

// ============================================================
// WavPlayer hooks
// ============================================================
//
// All three follow WA's "result-via-out-pointer + return-pointer-in-EAX"
// usercall convention. The Rust impl writes the sentinel into `*result`
// itself and returns `result` — cdecl puts it in EAX, which matches the
// WA caller's expectation.

pub(crate) unsafe extern "cdecl" fn wav_player_stop_impl(
    player: *mut WavPlayer,
    result: *mut u32,
) -> *mut u32 {
    unsafe {
        wav_player::wav_player_stop(player);
        *result = wav_player::wav_result_success();
        result
    }
}

pub(crate) unsafe extern "cdecl" fn wav_player_play_impl(
    result: *mut u32,
    player: *mut WavPlayer,
    flags: u32,
) -> *mut u32 {
    unsafe {
        wav_player::wav_player_play(player, flags);
        *result = wav_player::wav_result_success();
        result
    }
}

pub(crate) unsafe extern "cdecl" fn wav_player_load_and_play_impl(
    result: *mut u32,
    player: *mut WavPlayer,
    path: *const c_char,
    param3: i32,
) -> *mut u32 {
    unsafe {
        let ok = wav_player::wav_player_load_and_play(player, path, param3);
        *result = if ok {
            wav_player::wav_result_success()
        } else {
            0xFFFFFFFF
        };
        result
    }
}

// ============================================================
// PlayFeSfx hook (0x4D7960)
// ============================================================

pub(crate) unsafe extern "stdcall" fn hook_play_fe_sfx(sfx_name: *const c_char) {
    unsafe {
        let name = if !sfx_name.is_null() {
            CStr::from_ptr(sfx_name).to_str().unwrap_or("?")
        } else {
            "(null)"
        };
        speech_ops::play_fe_sfx(name);
    }
}

// ============================================================
// PlayFanfare_Default hook (0x4D7500)
// ============================================================

pub(crate) unsafe extern "stdcall" fn hook_play_fanfare_default(team_type: u32) {
    unsafe {
        speech_ops::play_fanfare_default(team_type);
    }
}

// ============================================================
// PlayFanfare_CurrentTeam hook (0x4D78E0)
// ============================================================

pub(crate) unsafe extern "cdecl" fn play_fanfare_current_team_impl(eax_index: u32) -> u32 {
    unsafe { speech_ops::play_fanfare_current_team(eax_index) }
}

// ============================================================
// DSSound_LoadSpeechBank hook (0x571660)
// ============================================================

pub(crate) unsafe extern "cdecl" fn load_speech_bank_impl(
    ddgw: *mut c_void,
    team_index: u32,
    speech_base_path: *const c_char,
    speech_dir: *const c_char,
) {
    unsafe {
        speech_ops::load_speech_bank(
            ddgw as *const GameRuntime,
            team_index,
            speech_base_path as *const u8,
            speech_dir as *const u8,
        );
    }
}

// ============================================================
// DSSound_LoadAllSpeechBanks hook (0x571A70)
// ============================================================

pub(crate) unsafe extern "cdecl" fn load_all_speech_banks_impl(ddgw: *mut c_void) {
    unsafe {
        speech_ops::load_all_speech_banks(ddgw as *const GameRuntime);
    }
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        // Global WavPlayer hooks — all callers (our hooks + unhooked WA code)
        // go through Rust. Avoids CRT heap mismatch.
        hooks::install_WavPlayer__Stop()?;
        hooks::install_WavPlayer__Play()?;
        hooks::install_WavPlayer__LoadAndPlay()?;

        hooks::install_PlayFeSfx()?;
        hooks::install_PlayFanfare_Default()?;
        hooks::install_PlayFanfare_CurrentTeam()?;
        hooks::install_DSSound__LoadSpeechBank()?;
        hooks::install_DSSound__LoadAllSpeechBanks()?;
    }

    Ok(())
}
