//! Sound playback replacements.
//!
//! Full Rust reimplementations of the two sound queue functions.
//! Enable logging with `OPENWA_SOUND_LOG=1` environment variable.
//!
//! Hooks:
//! - PlaySoundGlobal (0x546E20): __thiscall, ECX=CTask*, 4 stack params, RET 0x10
//! - PlaySoundLocal (0x4FDFE0): __usercall, EAX+ECX+EDI + 2 stack params, RET 0x8

use std::sync::atomic::Ordering;

use openwa_core::address::va;
use openwa_core::audio::{play_sound, play_sound_pooled, KnownSoundId, SoundId};
use openwa_core::engine::{DDGame, DDGameWrapper, SoundQueueEntry};
use openwa_core::fixed::Fixed;
use openwa_core::task::{CGameTask, CTask};

use crate::hook;
use crate::log_line;

/// Whether sound logging is enabled (checked once at init).
static SOUND_LOG_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ============================================================
// Core: sound queue insertion
// ============================================================

/// Insert a sound into DDGame's 16-slot queue.
///
/// Returns a pointer to the new entry, or None if the queue is full
/// or sound is disabled.
pub unsafe fn queue_sound(
    ddgame: *mut DDGame,
    sound_id: SoundId,
    flags: u32,
    volume: Fixed,
    pitch: Fixed,
) -> Option<*mut SoundQueueEntry> {
    let g = &mut *ddgame;
    if g.sound_queue_count >= 16 || g.sound.is_null() {
        return None;
    }
    let entry = &mut g.sound_queue[g.sound_queue_count as usize];
    *entry = SoundQueueEntry {
        sound_id: sound_id.0,
        flags,
        volume: volume.0 as u32,
        pitch: pitch.0 as u32,
        reserved: 0,
        is_local: 0,
        _pad: [0; 3],
        pos_x: 0,
        pos_y: 0,
        secondary_vtable: 0,
    };
    g.sound_queue_count += 1;
    Some(entry)
}

// ============================================================
// PlaySoundGlobal (0x546E20)
// ============================================================
// __thiscall: ECX = CTask* this, 4 stack params, RET 0x10

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

    queue_sound((*this).base.ddgame, SoundId(sound_id), flags, volume, pitch).is_some() as u32
}

// ============================================================
// PlaySoundLocal (0x4FDFE0)
// ============================================================
// __usercall: EAX=pitch, ECX=volume, EDI=task, stack[0]=sound_id, stack[1]=flags
// RET 0x8

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

    let gt = &*task;
    let ddgame = gt.base.ddgame;
    let entry = match queue_sound(ddgame, SoundId(sound_id), flags, volume, pitch) {
        Some(e) => e,
        None => return 0,
    };

    // Mark as local sound
    (*entry).is_local = 1;

    // Store pointer to task's sound emitter sub-object (CGameTask+0xE8)
    let emitter = &gt.sound_emitter;
    (*entry).secondary_vtable = emitter as *const _ as u32;

    // Call GetPosition(this, &pos_x, &pos_y) via typed vtable
    ((*emitter.vtable).get_position)(emitter, &mut (*entry).pos_x, &mut (*entry).pos_y);

    // Increment local sound count
    (*task).sound_emitter.local_sound_count += 1;

    1
}

/// Play a local sound on a task — Rust-callable version of PlaySoundLocal (0x4FDFE0).
///
/// Queues the sound, marks it as local, records the emitter position, and
/// increments the task's local sound count. Returns true on success.
pub(crate) unsafe fn play_sound_local(
    task: *mut CGameTask,
    sound_id: impl Into<SoundId>,
    flags: u32,
    volume: Fixed,
    pitch: Fixed,
) -> bool {
    let gt = &*task;
    let ddgame = gt.base.ddgame;
    let entry = match queue_sound(ddgame, sound_id.into(), flags, volume, pitch) {
        Some(e) => e,
        None => return false,
    };

    (*entry).is_local = 1;

    let emitter = &gt.sound_emitter;
    (*entry).secondary_vtable = emitter as *const _ as u32;
    ((*emitter.vtable).get_position)(emitter, &mut (*entry).pos_x, &mut (*entry).pos_y);

    (*task).sound_emitter.local_sound_count += 1;
    true
}

// ============================================================
// Sound dispatch helpers (bridge: queue → DSSound)
// ============================================================

/// IsSoundSuppressed (0x5261E0) — thiscall(ECX=DDGame*), plain RET.
/// Returns 0 if sound playback is allowed, 1 if suppressed.
unsafe extern "thiscall" fn hook_is_sound_suppressed(ddgame: *mut DDGame) -> u32 {
    let g = &*ddgame;
    let gi = &*g.game_info;
    if gi.sound_mute != 0 {
        return 1;
    }
    if g.frame_counter < gi.sound_start_frame {
        return 1;
    }
    0
}

/// DispatchGlobalSound (0x526270) — fastcall(ECX=unused, EDX=ddgame_wrapper) + 4 stack, RET 0x10.
/// Checks sound suppression, then calls DSSound vtable slot 3 (play_sound).
unsafe extern "fastcall" fn hook_dispatch_global_sound(
    _ecx: u32,
    ddgame_wrapper: *const DDGameWrapper,
    sound_id: u32,
    flags: u32,
    volume: u32,
    pitch: u32,
) -> u32 {
    let g = &*(*ddgame_wrapper).ddgame;
    let gi = &*g.game_info;

    // Suppression check — returns -1 if muted or before sound start frame
    if gi.sound_mute != 0 || g.frame_counter < gi.sound_start_frame {
        return 0xFFFF_FFFF;
    }

    let dssound = g.sound;
    if dssound.is_null() {
        return 0;
    }

    play_sound(
        dssound,
        sound_id,
        flags as i32,
        volume as i32,
        pitch as i32,
        0,
    ) as u32
}

/// PlaySoundPooled_Direct (0x546B50) — fastcall(ECX=unused, EDX=task) + 3 stack, RET 0xC.
/// Bypasses queue, checks suppression + fast-forward, calls DSSound vtable slot 4 directly.
unsafe extern "fastcall" fn hook_play_sound_pooled_direct(
    _ecx: u32,
    task: *const CTask,
    param1: u32,
    param2: u32,
    param3: u32,
) -> u32 {
    let g = &*(*task).ddgame;
    let gi = &*g.game_info;

    // Suppression check
    if gi.sound_mute != 0 || g.frame_counter < gi.sound_start_frame {
        return 0xFFFF_FFFF;
    }

    // Fast-forward check (unique to this function)
    if g.fast_forward_active != 0 {
        return 0xFFFF_FFFF;
    }

    let dssound = g.sound;
    if dssound.is_null() {
        return 0;
    }

    play_sound_pooled(dssound, param1, param2 as i32, 0x10000, param3 as i32, 0) as u32
}

// ============================================================
// Hook installation
// ============================================================

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

        // Sound dispatch helpers (bridge: queue → DSSound)
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

        // Patch DSSound vtable: replace trivial slots with Rust implementations.
        patch_dssound_vtable()?;
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
