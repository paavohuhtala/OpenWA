//! Speech / fanfare / FE SFX passthrough hooks.
//!
//! Logs speech-related function calls without replacing behavior.
//! All hooks call the original WA.exe function via MinHook trampoline.
//!
//! Hooks:
//! - PlayFeSfx (0x4D7960): stdcall(sfx_name), RET 0x4
//! - PlayFanfare_CurrentTeam (0x4D78E0): usercall(EAX=index), plain RET
//! - DSSound_LoadSpeechBank (0x571660): usercall(EAX=DDGame) + 3 stack, RET 0xC

use std::ffi::CStr;
use std::sync::atomic::{AtomicU32, Ordering};

use openwa_types::address::va;

use crate::hook::{self, usercall_trampoline};
use crate::log_line;

// ============================================================
// PlayFeSfx passthrough (0x4D7960)
// ============================================================
// stdcall(sfx_name: *const u8), RET 0x4

static ORIG_PLAY_FE_SFX: AtomicU32 = AtomicU32::new(0);

unsafe extern "stdcall" fn hook_play_fe_sfx(sfx_name: *const u8) {
    let name = if !sfx_name.is_null() {
        CStr::from_ptr(sfx_name as *const i8)
            .to_str()
            .unwrap_or("?")
    } else {
        "(null)"
    };
    let _ = log_line(&format!("[Speech] PlayFeSfx: \"{}\"", name));

    let orig: unsafe extern "stdcall" fn(*const u8) =
        core::mem::transmute(ORIG_PLAY_FE_SFX.load(Ordering::Relaxed) as usize);
    orig(sfx_name);
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
// DSSound_LoadSpeechBank passthrough (0x571660)
// ============================================================
// usercall(EAX=DDGame) + 3 stack params (team_index, speech_path, speech_dir)
// RET 0xC

static ORIG_LOAD_SPEECH_BANK: AtomicU32 = AtomicU32::new(0);

usercall_trampoline!(fn trampoline_load_speech_bank; impl_fn = load_speech_bank_impl;
    reg = eax; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn load_speech_bank_impl(
    ddgame: u32,
    team_index: u32,
    speech_path: *const u8,
    speech_dir: *const u8,
) {
    let path_str = if !speech_path.is_null() {
        CStr::from_ptr(speech_path as *const i8)
            .to_str()
            .unwrap_or("?")
    } else {
        "(null)"
    };
    let dir_str = if !speech_dir.is_null() {
        CStr::from_ptr(speech_dir as *const i8)
            .to_str()
            .unwrap_or("?")
    } else {
        "(null)"
    };
    let _ = log_line(&format!(
        "[Speech] LoadSpeechBank: team={} path=\"{}\" dir=\"{}\" ddgame=0x{:08X}",
        team_index, path_str, dir_str, ddgame
    ));

    // Call original via trampoline — must restore EAX and push stack params
    let orig = ORIG_LOAD_SPEECH_BANK.load(Ordering::Relaxed);
    core::arch::asm!(
        "push {dir}",
        "push {path}",
        "push {team}",
        "call {orig}",
        team = in(reg) team_index,
        path = in(reg) speech_path,
        dir = in(reg) speech_dir,
        orig = in(reg) orig,
        in("eax") ddgame,
        clobber_abi("C"),
    );
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let trampoline = hook::install(
            "PlayFeSfx",
            va::PLAY_FE_SFX,
            hook_play_fe_sfx as *const (),
        )?;
        ORIG_PLAY_FE_SFX.store(trampoline as u32, Ordering::Relaxed);

        let trampoline = hook::install(
            "PlayFanfare_CurrentTeam",
            va::PLAY_FANFARE_CURRENT_TEAM,
            trampoline_play_fanfare_current_team as *const (),
        )?;
        ORIG_PLAY_FANFARE_CURRENT_TEAM.store(trampoline as u32, Ordering::Relaxed);

        let trampoline = hook::install(
            "DSSound_LoadSpeechBank",
            va::DSSOUND_LOAD_SPEECH_BANK,
            trampoline_load_speech_bank as *const (),
        )?;
        ORIG_LOAD_SPEECH_BANK.store(trampoline as u32, Ordering::Relaxed);
    }

    Ok(())
}
