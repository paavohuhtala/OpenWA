//! Sound playback hooks.
//!
//! Thin wormkit shim — game logic lives in `openwa_core::audio::sound_ops`.
//! This file contains hook entry points, trampolines, and installation.
//!
//! Hooks:
//! - PlaySoundGlobal (0x546E20): __thiscall, ECX=CTask*, 4 stack params, RET 0x10
//! - PlaySoundLocal (0x4FDFE0): __usercall, EAX+ECX+EDI + 2 stack params, RET 0x8
//! - IsSoundSuppressed (0x5261E0): __thiscall, ECX=DDGame*
//! - DispatchGlobalSound (0x526270): __fastcall + 4 stack, RET 0x10
//! - PlaySoundPooled_Direct (0x546B50): __fastcall + 3 stack, RET 0xC
//! - WormPlaySound2 (0x515020): __usercall(EDI=worm) + 3 stack, RET 0xC

use std::sync::atomic::Ordering;

use openwa_core::address::va;
use openwa_core::audio::sound_ops;
use openwa_core::audio::{KnownSoundId, SoundId, SoundSlot};
use openwa_core::engine::{DDGame, DDGameWrapper};
use openwa_core::fixed::Fixed;
use openwa_core::task::worm::CTaskWorm;
use openwa_core::task::{CGameTask, CTask};

use crate::hook;
use crate::log_line;

/// Whether sound logging is enabled (checked once at init).
static SOUND_LOG_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ── PlaySoundGlobal (0x546E20): thiscall(ECX=CTask*, 4 stack, RET 0x10) ──

unsafe extern "thiscall" fn hook_play_sound_global(
    this: *const CGameTask,
    sound_id: u32,
    flags: u32,
    volume: Fixed,
    pitch: Fixed,
) -> u32 {
    if SOUND_LOG_ENABLED.load(Ordering::Relaxed) {
        let sound_name = KnownSoundId::try_from(sound_id)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|v| format!("#{v}"));
        let _ = log_line(&format!(
            "[Sound] Global: task=0x{this:08X?} id={sound_id}({sound_name}) \
             p3={flags} p4={volume} p5={pitch}"
        ));
    }

    sound_ops::queue_sound((*this).base.ddgame, SoundId(sound_id), flags, volume, pitch).is_some()
        as u32
}

// ── PlaySoundLocal (0x4FDFE0): usercall(EAX=pitch, ECX=volume, EDI=task, stack) ──

hook::usercall_trampoline!(fn trampoline_play_sound_local; impl_fn = play_sound_local_impl;
    regs = [eax, ecx, edi]; stack_params = 2; ret_bytes = "0x8");

unsafe extern "cdecl" fn play_sound_local_impl(
    pitch: Fixed,
    volume: Fixed,
    task: *mut CGameTask,
    sound_id: u32,
    flags: u32,
) -> u32 {
    if SOUND_LOG_ENABLED.load(Ordering::Relaxed) {
        let sound_name = KnownSoundId::try_from(sound_id)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|v| format!("#{v}"));
        let _ = log_line(&format!(
            "[Sound] Local: pitch={pitch} volume={volume} task=0x{task:08X?} \
             id={sound_id}({sound_name}) flags={flags}"
        ));
    }

    sound_ops::play_sound_local(task, SoundId(sound_id), flags, volume, pitch) as u32
}

// ── WormPlaySound2 (0x515020): usercall(EDI=worm) + stdcall(sound_id, volume, flags) ──

#[unsafe(naked)]
unsafe extern "C" fn trampoline_worm_play_sound_2() {
    core::arch::naked_asm!(
        "push [esp+12]",   // flags
        "push [esp+12]",   // volume (was +8, shifted +4)
        "push [esp+12]",   // sound_id (was +4, shifted +8)
        "push edi",        // worm
        "call {f}",
        "add esp, 16",     // clean cdecl args
        "ret 0xC",         // clean 3 stdcall params
        f = sym play_worm_sound_2_cdecl,
    );
}

unsafe extern "cdecl" fn play_worm_sound_2_cdecl(
    worm: *mut CTaskWorm,
    sound_id: u32,
    volume: u32,
    flags: u32,
) {
    sound_ops::play_worm_sound_2(worm, SoundId(sound_id), Fixed(volume as i32), flags);
}

// ── IsSoundSuppressed (0x5261E0): thiscall(ECX=DDGame*) ──

unsafe extern "thiscall" fn hook_is_sound_suppressed(ddgame: *mut DDGame) -> u32 {
    sound_ops::is_sound_suppressed(ddgame) as u32
}

// ── DispatchGlobalSound (0x526270): fastcall(ECX=unused, EDX=wrapper) + 4 stack ──

unsafe extern "fastcall" fn hook_dispatch_global_sound(
    _ecx: u32,
    ddgame_wrapper: *const DDGameWrapper,
    sound_id: u32,
    flags: u32,
    volume: u32,
    pitch: u32,
) -> u32 {
    sound_ops::dispatch_global_sound(ddgame_wrapper, sound_id, flags, volume, pitch)
}

// ── PlaySoundPooled_Direct (0x546B50): fastcall(ECX=unused, EDX=task) + 3 stack ──

unsafe extern "fastcall" fn hook_play_sound_pooled_direct(
    _ecx: u32,
    task: *const CTask,
    param1: SoundSlot,
    param2: i32,
    param3: Fixed,
) -> i32 {
    sound_ops::play_sound_pooled_direct(task, param1, param2, param3)
}

// ── Hook installation ──

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_SOUND_LOG").is_ok() {
        SOUND_LOG_ENABLED.store(true, Ordering::Relaxed);
        let _ = log_line("[Sound] Logging enabled (OPENWA_SOUND_LOG=1)");
    }

    unsafe {
        let _ = hook::install(
            "PlaySoundGlobal",
            va::PLAY_SOUND_GLOBAL,
            hook_play_sound_global as *const (),
        )?;
        let _ = hook::install(
            "PlaySoundLocal",
            va::PLAY_SOUND_LOCAL,
            trampoline_play_sound_local as *const (),
        )?;
        let _ = hook::install(
            "IsSoundSuppressed",
            va::IS_SOUND_SUPPRESSED,
            hook_is_sound_suppressed as *const (),
        )?;
        let _ = hook::install(
            "DispatchGlobalSound",
            va::DISPATCH_GLOBAL_SOUND,
            hook_dispatch_global_sound as *const (),
        )?;
        let _ = hook::install(
            "PlaySoundPooled_Direct",
            va::PLAY_SOUND_POOLED_DIRECT,
            hook_play_sound_pooled_direct as *const (),
        )?;

        // Patch DSSound vtable: replace all 24 slots with Rust implementations.
        patch_dssound_vtable()?;

        // Hook CTaskWorm::PlaySound2 (FUN_00515020) — 23 callers in WA
        let _ = hook::install(
            "WormPlaySound2",
            va::WORM_PLAY_SOUND_2,
            trampoline_worm_play_sound_2 as *const (),
        )?;

        // Initialize bridge address for streaming sound functions
        sound_ops::LOAD_AND_PLAY_STREAMING_ADDR.store(
            openwa_core::rebase::rb(va::LOAD_AND_PLAY_STREAMING),
            core::sync::atomic::Ordering::Relaxed,
        );
    }

    Ok(())
}

/// Patch DSSound vtable (0x66AF20) to replace trivial methods with Rust.
unsafe fn patch_dssound_vtable() -> Result<(), &'static str> {
    use openwa_core::audio::{
        dssound_destructor, dssound_noop, dssound_returns_0, dssound_returns_1,
        dssound_sub_destructor, is_channel_finished, is_slot_loaded, load_wav, play_sound,
        play_sound_pooled, release_finished, set_channel_volume, set_master_volume, set_pan,
        set_volume_params, stop_channel, update_channels, DSSoundVtable,
    };
    use openwa_core::vtable_replace;

    vtable_replace!(DSSoundVtable, va::DS_SOUND_VTABLE, {
        destructor          => dssound_destructor,
        update_channels     => update_channels,
        set_volume_params   => set_volume_params,
        play_sound          => play_sound,
        play_sound_pooled   => play_sound_pooled,
        set_pan             => set_pan,
        stub_6              => dssound_returns_0,
        set_master_volume   => set_master_volume,
        set_channel_volume  => set_channel_volume,
        is_channel_finished => is_channel_finished,
        stop_channel        => stop_channel,
        release_finished    => release_finished,
        load_wav            => load_wav,
        is_slot_loaded      => is_slot_loaded,
        sub_destructor      => dssound_sub_destructor,
        noop_15             => dssound_noop,
        noop_16             => dssound_noop,
        returns_0_17        => dssound_returns_0,
        returns_0_18        => dssound_returns_0,
        stub_19             => dssound_returns_0,
        stub_20             => dssound_returns_0,
        returns_0_21        => dssound_returns_0,
        stub_22             => dssound_returns_0,
        returns_1_23        => dssound_returns_1,
    })?;

    let _ = log_line("[Sound]   DSSound vtable: patched 24/24 slots with Rust");
    Ok(())
}
