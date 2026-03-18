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
use openwa_core::audio::{play_sound, play_sound_pooled, SoundId};
use openwa_core::engine::{DDGame, DDGameWrapper, SoundQueueEntry};
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
unsafe fn queue_sound(
    ddgame: *mut DDGame,
    sound_id: u32,
    flags: u32,
    volume: u32,
    pitch: u32,
) -> Option<*mut SoundQueueEntry> {
    let g = &mut *ddgame;
    if g.sound_queue_count >= 16 || g.sound.is_null() {
        return None;
    }
    let entry = &mut g.sound_queue[g.sound_queue_count as usize];
    *entry = SoundQueueEntry {
        sound_id,
        flags,
        volume,
        pitch,
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
    volume: u32,
    pitch: u32,
) -> u32 {
    if SOUND_LOG_ENABLED.load(Ordering::Relaxed) {
        let sound_name = SoundId::try_from(sound_id)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|v| format!("#{v}"));
        let _ = log_line(&format!(
            "[Sound] Global: task=0x{this:08X?} id={sound_id}({sound_name}) \
             p3={flags} p4={volume} p5={pitch}"
        ));
    }

    queue_sound((*this).base.ddgame, sound_id, flags, volume, pitch).is_some() as u32
}

// ============================================================
// PlaySoundLocal (0x4FDFE0)
// ============================================================
// __usercall: EAX=pitch, ECX=volume, EDI=task, stack[0]=sound_id, stack[1]=flags
// RET 0x8

hook::usercall_trampoline!(fn trampoline_play_sound_local; impl_fn = play_sound_local_impl;
    regs = [eax, ecx, edi]; stack_params = 2; ret_bytes = "0x8");

unsafe extern "cdecl" fn play_sound_local_impl(
    pitch: u32,
    volume: u32,
    task: *mut CGameTask,
    sound_id: u32,
    flags: u32,
) -> u32 {
    if SOUND_LOG_ENABLED.load(Ordering::Relaxed) {
        let sound_name = SoundId::try_from(sound_id)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|v| format!("#{v}"));
        let _ = log_line(&format!(
            "[Sound] Local: eax={pitch} ecx=0x{volume:08X} task=0x{task:08X?} \
             id={sound_id}({sound_name}) flags={flags}"
        ));
    }

    let gt = &*task;
    let ddgame = gt.base.ddgame;
    let entry = match queue_sound(ddgame, sound_id, flags, volume, pitch) {
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
unsafe fn patch_dssound_vtable() -> Result<(), String> {
    use openwa_core::audio::{
        dssound_destructor, dssound_noop, dssound_returns_0, dssound_returns_1,
        dssound_sub_destructor, is_channel_finished, is_slot_loaded, load_wav, play_sound,
        play_sound_pooled, release_finished, set_channel_volume, set_master_volume, set_pan,
        set_volume_params, stop_channel, update_channels,
    };
    use openwa_core::rebase::rb;
    use openwa_core::vtable::patch_vtable;

    let vtable = rb(va::DS_SOUND_VTABLE) as *mut u32;

    patch_vtable(vtable, 24, |vt| {
        // Slot 12: load_wav — WAV file → DirectSound secondary buffer (hound + windows crate)
        *vt.add(12) = load_wav as *const () as u32;

        // Slot 0: destructor — releases all COM objects
        *vt.add(0) = dssound_destructor as *const () as u32;

        // Slot 1: update_channels — release finished buffers each frame
        *vt.add(1) = update_channels as *const () as u32;

        // Slot 7: set_master_volume — adjusts all active channels
        *vt.add(7) = set_master_volume as *const () as u32;

        // Slot 8: set_channel_volume — sets volume on specific channel
        *vt.add(8) = set_channel_volume as *const () as u32;

        // Slot 11: release_finished — like update_channels, returns count
        *vt.add(11) = release_finished as *const () as u32;

        // Slot 3: play_sound — thiscall + 5 stack params
        *vt.add(3) = play_sound as *const () as u32;

        // Slot 4: play_sound_pooled — thiscall + 5 stack params
        *vt.add(4) = play_sound_pooled as *const () as u32;

        // Slot 2: set_volume_params — frequency scaling for all channels
        *vt.add(2) = set_volume_params as *const () as u32;

        // Slot 5: set_pan — stereo panning with dB lookup
        *vt.add(5) = set_pan as *const () as u32;

        // Slot 9: is_channel_finished — returns 0 if playing, 1 if stopped/free
        *vt.add(9) = is_channel_finished as *const () as u32;

        // Slot 10: stop_channel — stops buffer, releases, returns to pool
        *vt.add(10) = stop_channel as *const () as u32;

        // Slot 13: is_slot_loaded — channel_slots check
        *vt.add(13) = is_slot_loaded as *const () as u32;

        // Slot 14: sub_destructor — sets secondary vtable
        *vt.add(14) = dssound_sub_destructor as *const () as u32;

        // Trivial noops (slots 15, 16)
        *vt.add(15) = dssound_noop as *const () as u32;
        *vt.add(16) = dssound_noop as *const () as u32;

        // Trivial returns-0 (slots 6, 17, 18, 19, 20, 21, 22)
        for slot in [6, 17, 18, 19, 20, 21, 22] {
            *vt.add(slot) = dssound_returns_0 as *const () as u32;
        }

        // Trivial returns-1 (slot 23)
        *vt.add(23) = dssound_returns_1 as *const () as u32;

        let _ = log_line("[Sound]   DSSound vtable: patched 24/24 slots with Rust");
    })
    .map_err(|e| e.to_string())
}
