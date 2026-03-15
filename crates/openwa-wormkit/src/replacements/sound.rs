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
use openwa_core::engine::{DDGame, SoundQueueEntry};
use openwa_core::audio::SoundId;
use openwa_core::task::CGameTask;

use crate::hook;
use crate::log_line;

/// Whether sound logging is enabled (checked once at init).
static SOUND_LOG_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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
    this: u32,
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
            "[Sound] Global: task=0x{this:08X} id={sound_id}({sound_name}) \
             p3={flags} p4={volume} p5={pitch}"
        ));
    }

    let task = &*(this as *const CGameTask);
    queue_sound(task.base.ddgame, sound_id, flags, volume, pitch).is_some() as u32
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
    task: u32,
    sound_id: u32,
    flags: u32,
) -> u32 {
    if SOUND_LOG_ENABLED.load(Ordering::Relaxed) {
        let sound_name = SoundId::try_from(sound_id)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|v| format!("#{v}"));
        let _ = log_line(&format!(
            "[Sound] Local: eax={pitch} ecx=0x{volume:08X} task=0x{task:08X} \
             id={sound_id}({sound_name}) flags={flags}"
        ));
    }

    let gt = &*(task as *const CGameTask);
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
    let gt_mut = &mut *(task as *mut CGameTask);
    gt_mut.sound_emitter.local_sound_count += 1;

    1
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

        // Patch DSSound vtable: replace trivial slots with Rust implementations.
        patch_dssound_vtable()?;
    }

    Ok(())
}

/// Patch DSSound vtable (0x66AF20) to replace trivial methods with Rust.
unsafe fn patch_dssound_vtable() -> Result<(), String> {
    use openwa_core::rebase::rb;
    use openwa_core::vtable::patch_vtable;
    use openwa_core::audio::{
        update_channels, release_finished,
        is_slot_loaded, is_channel_playing, stop_channel, dssound_sub_destructor,
        load_wav, dssound_noop, dssound_returns_0, dssound_returns_1,
    };

    let vtable = rb(va::DS_SOUND_VTABLE) as *mut u32;

    patch_vtable(vtable, 24, |vt| {
        // Slot 12: load_wav — WAV file → DirectSound secondary buffer (hound + windows crate)
        *vt.add(12) = load_wav as *const () as u32;

        // Slots 1, 11 disabled — update_channels/release_finished crash
        // when iterating descriptors. Layout is confirmed correct via
        // runtime dumps; issue is likely COM refcount handling in buffer().
        // *vt.add(1) = update_channels as *const () as u32;
        // *vt.add(11) = release_finished as *const () as u32;

        // Slots 9, 10 disabled — slot 9's return value semantics are unclear
        // from decompile; wrong values break sound scheduling entirely.
        // *vt.add(9) = is_channel_playing as *const () as u32;

        // Slot 10: disabled pending slot 9 fix
        // *vt.add(10) = stop_channel as *const () as u32;

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

        let _ = log_line("[Sound]   DSSound vtable: patched 13/24 slots with Rust");
    }).map_err(|e| e.to_string())
}
