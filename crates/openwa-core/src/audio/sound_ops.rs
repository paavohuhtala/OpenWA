//! Sound playback operations.
//!
//! Pure Rust reimplementations of WA.exe sound functions. Called from
//! wormkit hook trampolines and from other core game logic (weapon fire, etc.).
//!
//! Original WA functions:
//! - PlaySoundGlobal queue insertion (0x546E20)
//! - PlaySoundLocal (0x4FDFE0)
//! - StopWormSound (FUN_00515180)
//! - PlayWormSound (FUN_005150D0)
//! - PlayWormSound2 (FUN_00515020)
//! - IsSoundSuppressed (0x5261E0)
//! - DispatchGlobalSound (0x526270)
//! - PlaySoundPooled_Direct (0x546B50)

use core::sync::atomic::{AtomicU32, Ordering};

use crate::audio::{play_sound, play_sound_pooled, SoundId, SoundSlot};
use crate::engine::{DDGame, DDGameWrapper, SoundQueueEntry};
use crate::fixed::Fixed;
use crate::task::worm::CTaskWorm;
use crate::task::{CGameTask, CTask};

// ============================================================
// Sound queue insertion
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
// PlaySoundLocal — Rust-callable version
// ============================================================

/// Play a local sound on a task — port of PlaySoundLocal (0x4FDFE0).
///
/// Queues the sound, marks it as local, records the emitter position, and
/// increments the task's local sound count. Returns true on success.
pub unsafe fn play_sound_local(
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
// Worm sound functions (streaming sound handle at CTaskWorm+0x3B0)
// ============================================================

/// Stop the worm's active streaming sound. Port of FUN_00515180.
///
/// Reads the sound handle from `worm.sound_handle`, dispatches to either
/// DSSound::stop_channel (regular) or ActiveSoundTable::stop_sound (streaming,
/// handle has bit 30 set), then clears the handle.
pub unsafe fn stop_worm_sound(worm: *mut CTaskWorm) {
    let handle = (*worm).sound_handle;
    if handle != 0 {
        let ddgame = CTask::ddgame_raw(worm as *const CTask);
        let sound = (*ddgame).sound;
        if !sound.is_null() && (handle as i32) >= 0 {
            if handle & 0x40000000 != 0 {
                // Streaming sound — stop via ActiveSoundTable
                if !(*ddgame).active_sounds.is_null() {
                    (*(*ddgame).active_sounds).stop_sound(handle & !0x40000000);
                }
            } else {
                // Regular DSSound channel
                ((*(*sound).vtable).stop_channel)(sound, handle as i32);
            }
        }
    }
    (*worm).sound_handle = 0;
}

/// Stop current worm sound, then start a new streaming sound.
/// Port of FUN_005150D0.
///
/// Stops the current sound (same logic as [`stop_worm_sound`] but with
/// reversed condition order matching the original), then calls the WA
/// streaming load-and-play function (FUN_00546c20) and stores the new handle.
pub unsafe fn play_worm_sound(worm: *mut CTaskWorm, sound_id: SoundId, volume: Fixed) {
    let handle = (*worm).sound_handle;
    if handle != 0 {
        let ddgame = CTask::ddgame_raw(worm as *const CTask);
        let sound = (*ddgame).sound;
        if !sound.is_null() && (handle as i32) >= 0 {
            if handle & 0x40000000 == 0 {
                // Regular DSSound channel — stop via vtable
                ((*(*sound).vtable).stop_channel)(sound, handle as i32);
            } else {
                // Streaming sound — stop via ActiveSoundTable
                if !(*ddgame).active_sounds.is_null() {
                    (*(*ddgame).active_sounds).stop_sound(handle & !0x40000000);
                }
            }
        }
    }
    // Start new streaming sound via WA function (FUN_00546c20)
    // FUN_005150D0 hardcodes flags=3
    let new_handle = call_load_and_play_streaming(worm, sound_id.0, 3, volume.0 as u32);
    (*worm).sound_handle = new_handle;
}

/// Stop+play on the secondary sound handle (CTaskWorm+0x3B4).
/// Port of FUN_00515020 (23 callers in WA).
///
/// Stops any active sound on `sound_handle_2`, then plays a new streaming
/// sound. Has a special case for sound 0x36 (Teleport) when the worm's Y
/// position is extremely high — plays at weapon target position instead.
pub unsafe fn play_worm_sound_2(
    worm: *mut CTaskWorm,
    sound_id: SoundId,
    volume: Fixed,
    flags: u32,
) {
    // 1. Stop current sound on handle_2
    let handle = (*worm).sound_handle_2;
    if handle != 0 {
        let ddgame = CTask::ddgame_raw(worm as *const CTask);
        let sound = (*ddgame).sound;
        if !sound.is_null() && (handle as i32) >= 0 {
            if handle & 0x40000000 == 0 {
                ((*(*sound).vtable).stop_channel)(sound, handle as i32);
            } else if !(*ddgame).active_sounds.is_null() {
                (*(*ddgame).active_sounds).stop_sound(handle & !0x40000000);
            }
        }
    }

    // 2. Start new sound
    // CGameTask+0x88 = Y position (fixed-point). Check if worm is extremely high.
    let worm_y = *((worm as *const u8).add(0x88) as *const i32);
    let new_handle = if worm_y < -0x270F_FFFF && sound_id.0 == 0x36 {
        // Special teleport case: play at weapon target position
        call_load_and_play_streaming_positional(
            worm,
            sound_id.0,
            flags,
            volume.0 as u32,
            (*worm).weapon_param_1 as u32,
            (*worm).weapon_param_2 as u32,
        )
    } else {
        // Normal case: play streaming sound
        call_load_and_play_streaming(worm, sound_id.0, flags, volume.0 as u32)
    };

    (*worm).sound_handle_2 = new_handle;
}

// ============================================================
// Sound dispatch helpers
// ============================================================

/// Check if sound playback is suppressed — port of IsSoundSuppressed (0x5261E0).
///
/// Returns true if sound is muted or current frame is before sound start.
pub unsafe fn is_sound_suppressed(ddgame: *const DDGame) -> bool {
    let g = &*ddgame;
    let gi = &*g.game_info;
    gi.sound_mute != 0 || g.frame_counter < gi.sound_start_frame
}

/// Dispatch a global sound to DSSound — port of DispatchGlobalSound (0x526270).
///
/// Checks sound suppression, then calls DSSound play_sound.
/// Returns 0xFFFFFFFF if suppressed, 0 if no DSSound, otherwise the channel handle.
pub unsafe fn dispatch_global_sound(
    ddgame_wrapper: *const DDGameWrapper,
    sound_id: u32,
    flags: u32,
    volume: u32,
    pitch: u32,
) -> u32 {
    let g = &*(*ddgame_wrapper).ddgame;
    let gi = &*g.game_info;

    if gi.sound_mute != 0 || g.frame_counter < gi.sound_start_frame {
        return 0xFFFF_FFFF;
    }

    let dssound = g.sound;
    if dssound.is_null() {
        return 0;
    }

    play_sound(
        dssound,
        SoundSlot(sound_id),
        flags as i32,
        Fixed(volume as i32),
        Fixed(pitch as i32),
        Fixed(0),
    ) as u32
}

/// Direct pooled sound playback — port of PlaySoundPooled_Direct (0x546B50).
///
/// Bypasses queue, checks suppression + fast-forward, calls DSSound play_sound_pooled.
pub unsafe fn play_sound_pooled_direct(
    task: *const CTask,
    param1: SoundSlot,
    param2: i32,
    param3: Fixed,
) -> i32 {
    let g = &*(*task).ddgame;
    let gi = &*g.game_info;

    if gi.sound_mute != 0 || g.frame_counter < gi.sound_start_frame {
        return -1;
    }

    if g.fast_forward_active != 0 {
        return -1;
    }

    let dssound = g.sound;
    if dssound.is_null() {
        return 0;
    }

    play_sound_pooled(dssound, param1, param2, Fixed::ONE, param3, Fixed::ZERO)
}

// ============================================================
// Streaming sound bridges (naked asm — WA usercall conventions)
// ============================================================

/// Rebased address for FUN_00546c20 (LoadAndPlayStreaming).
/// Initialized by wormkit's sound::install().
pub static LOAD_AND_PLAY_STREAMING_ADDR: AtomicU32 = AtomicU32::new(0);

/// Bridge to FUN_00546c20 — load and play a streaming sound.
/// usercall(EAX=worm, ESI=&worm.sound_emitter) + stdcall(sound_id, flags, volume), plain RET.
/// Returns new sound handle (with bit 30 set for streaming, 0 on failure, -1 if suppressed).
pub unsafe fn call_load_and_play_streaming(
    worm: *mut CTaskWorm,
    sound_id: u32,
    flags: u32,
    volume: u32,
) -> u32 {
    let addr = LOAD_AND_PLAY_STREAMING_ADDR.load(Ordering::Relaxed);
    call_load_and_play_streaming_bridge(worm, sound_id, flags, volume, addr)
}

#[unsafe(naked)]
unsafe extern "C" fn call_load_and_play_streaming_bridge(
    _worm: *mut CTaskWorm,
    _sound_id: u32,
    _flags: u32,
    _volume: u32,
    _addr: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push esi",
        "push ebx",
        // Stack: 2 saves(8) + ret(4) = 12 to first arg
        "mov eax, [esp+12]",   // worm → EAX
        "lea esi, [eax+0xE8]", // &worm.sound_emitter → ESI
        "mov ebx, [esp+28]",   // addr (12 + 4*4 = 28)
        "push [esp+24]",       // volume (12+12=24)
        "push [esp+20]",       // flags (12+8=20, shifted+4=24)
        "push [esp+24]",       // sound_id (12+4=16, shifted+8=24)
        "call ebx",
        "pop ebx",
        "pop esi",
        "ret",
    );
}

/// Bridge to FUN_00546bb0 — load and play a positional streaming sound.
/// usercall(EAX=worm, ESI=&worm.sound_emitter) + stdcall(sound_id, flags, volume, x, y).
/// Returns handle | 0x40000000 on success, 0 on failure, -1 if suppressed.
pub unsafe fn call_load_and_play_streaming_positional(
    worm: *mut CTaskWorm,
    sound_id: u32,
    flags: u32,
    volume: u32,
    x: u32,
    y: u32,
) -> u32 {
    let addr = crate::rebase::rb(0x546BB0u32);
    call_load_and_play_streaming_positional_bridge(worm, sound_id, flags, volume, x, y, addr)
}

#[unsafe(naked)]
unsafe extern "C" fn call_load_and_play_streaming_positional_bridge(
    _worm: *mut CTaskWorm,
    _sound_id: u32,
    _flags: u32,
    _volume: u32,
    _x: u32,
    _y: u32,
    _addr: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push esi",
        "push ebx",
        // Stack after saves: [ESP+12]=worm [+16]=sound_id [+20]=flags
        //   [+24]=volume [+28]=x [+32]=y [+36]=addr
        "mov eax, [esp+12]",   // worm → EAX
        "lea esi, [eax+0xE8]", // &worm.sound_emitter → ESI
        "mov ebx, [esp+36]",   // addr
        // Push 5 stdcall params right-to-left: y, x, volume, flags, sound_id.
        "push [esp+32]", // y
        "push [esp+32]", // x (was +28, shifted +4)
        "push [esp+32]", // volume (was +24, shifted +8)
        "push [esp+32]", // flags (was +20, shifted +12)
        "push [esp+32]", // sound_id (was +16, shifted +16)
        "call ebx",
        "pop ebx",
        "pop esi",
        "ret",
    );
}
