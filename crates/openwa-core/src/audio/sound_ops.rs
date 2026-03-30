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

use crate::audio::{play_sound, play_sound_pooled, SoundId};
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
    // Start new streaming sound — fully ported, no WA bridge needed
    // FUN_005150D0 hardcodes flags=3
    let new_handle = load_and_play_streaming(worm, sound_id, 3, volume);
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
        load_and_play_streaming_positional(
            worm,
            sound_id,
            flags,
            volume,
            Fixed((*worm).weapon_param_1 as i32),
            Fixed((*worm).weapon_param_2 as i32),
        )
    } else {
        // Normal case: play streaming sound — fully ported
        load_and_play_streaming(worm, sound_id, flags, volume)
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
    slot: SoundId,
    priority: i32,
    frequency: Fixed,
    volume: Fixed,
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

    play_sound(dssound, slot, priority, frequency, volume, Fixed::ZERO) as u32
}

/// Direct pooled sound playback — port of PlaySoundPooled_Direct (0x546B50).
///
/// Bypasses queue, checks suppression + fast-forward, calls DSSound play_sound_pooled.
pub unsafe fn play_sound_pooled_direct(
    task: *const CTask,
    slot: SoundId,
    priority: i32,
    volume: Fixed,
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

    play_sound_pooled(dssound, slot, priority, Fixed::ONE, volume, Fixed::ZERO)
}

// ============================================================
// Local/streaming sound — full Rust port
// ============================================================
//
// Replaces the former naked asm bridges to WA's LoadAndPlayStreaming (0x546C20)
// and LoadAndPlayStreamingPositional (0x546BB0) with pure Rust implementations
// of the entire call chain:
//
//   load_and_play_streaming → dispatch_local_sound
//     → compute_distance_params → distance_3d_attenuation
//     → DSSound::play_sound_pooled (vtable, already Rust)
//     → ActiveSoundTable::record_sound

use crate::audio::active_sound::ActiveSoundTable;

/// 3D stereo distance attenuation — port of Distance3D_Attenuation (0x5430F0).
///
/// Computes volume attenuation and stereo pan based on the distance between
/// a sound source and the listener, using an elliptical model.
///
/// The function models two "ears" at ±half_width from the listener's X position.
/// For each ear, it computes an inverse-distance attenuation from the sound source
/// using: `attenuation = scaled² / (scaled² + dy² + dx_ear²)`, where `scaled` is
/// the characteristic distance (abs(half_width) * 0xDDB3 / 0x10000 ≈ 0.866 * half_width).
///
/// Returns `(volume, pan)` as Fixed values where:
/// - `volume` = max(left_atten, right_atten), clamped to Fixed::ONE
/// - `pan` = right_atten - left_atten (positive = sound is to the right)
///
/// Convention: EAX = &listener_pos[x,y], stdcall(sound_x, sound_y, level_width_fixed,
///   attenuation_factor, out_volume, out_pan), RET 0x18.
fn distance_3d_attenuation(
    listener_x: Fixed,
    listener_y: Fixed,
    sound_x: Fixed,
    sound_y: Fixed,
    level_width_fixed: Fixed,
    attenuation_factor: i32,
) -> (Fixed, Fixed) {
    /// √3/2 ≈ 0.866, stored as Fixed (0xDDB3 / 0x10000)
    const SQRT3_HALF: Fixed = Fixed(0xDDB3);

    // Step 1: half_width = level_width / attenuation_factor
    // Uses 64-bit intermediate, clamped so abs() can't overflow i32
    let wide = (level_width_fixed.0 as i64) * (Fixed::ONE.0 as i64);
    let half_width_64 = wide / (attenuation_factor as i64);
    let half_width = Fixed(if half_width_64 > 0x7FFFFFFF {
        0x7FFFFFFF
    } else if half_width_64 < -0x7FFFFFFF {
        // Clamp to -0x7FFFFFFF (not i32::MIN) so abs() in step 2 can't overflow
        -0x7FFFFFFF
    } else {
        half_width_64 as i32
    });

    // Step 2: characteristic distance = abs(half_width) * √3/2
    let scaled = half_width.abs() * SQRT3_HALF;

    // Steps 3-5 use i64 intermediates to match the original's __allmul/__alldiv
    // precision. The original keeps scaled_sq/dy_sq as full 64-bit values through
    // the attenuation computation; truncating to i32 (Fixed) would lose precision.
    let scaled_i = scaled.0 as i64;
    let dy_i = (sound_y.0 - listener_y.0) as i64;
    let dx = sound_x - listener_x;
    let half = half_width / 2;

    // squared terms: a² / ONE (fixed-point squares, 64-bit)
    let scaled_sq = (scaled_i * scaled_i) / Fixed::ONE.0 as i64;
    let dy_sq = (dy_i * dy_i) / Fixed::ONE.0 as i64;

    // Compute per-ear inverse-distance attenuation:
    //   atten = (scaled_sq * ONE) / (scaled_sq + dy_sq + dx_ear²)

    // Step 4: Left ear — distance from sound to left ear = -(half + dx)
    let left_dist = -(half + dx).0 as i64;
    let left_dist_sq = (left_dist * left_dist) / Fixed::ONE.0 as i64;
    let left_denom = scaled_sq + dy_sq + left_dist_sq;
    let left_atten = if left_denom == 0 {
        Fixed::ONE
    } else {
        Fixed(((scaled_sq * Fixed::ONE.0 as i64) / left_denom) as i32).min(Fixed::ONE)
    };

    // Step 5: Right ear — distance from sound to right ear = half - dx
    let right_dist = (half - dx).0 as i64;
    let right_dist_sq = (right_dist * right_dist) / Fixed::ONE.0 as i64;
    let right_denom = right_dist_sq + dy_sq + scaled_sq;
    let right_atten = if right_denom == 0 {
        Fixed::ONE
    } else {
        Fixed(((scaled_sq * Fixed::ONE.0 as i64) / right_denom) as i32).min(Fixed::ONE)
    };

    // Step 6: volume = max(left, right), pan = right - left
    (left_atten.max(right_atten), right_atten - left_atten)
}

/// Compute distance-based volume and pan for a local sound — port of
/// ComputeDistanceParams (0x546300).
///
/// Reads the attenuation factor from GameInfo and the level width from DDGame.
/// If attenuation is disabled (factor == 0), returns full volume and center pan.
///
/// Convention: fastcall(ECX=&out_pan, EDX=&out_volume, stack=[table, x, y]), RET 0xC.
unsafe fn compute_distance_params(ddgame: *const DDGame, x: Fixed, y: Fixed) -> (Fixed, Fixed) {
    let gi = &*(*ddgame).game_info;
    let attenuation = gi.sound_attenuation;

    if attenuation == 0 {
        // No 3D audio — full volume, center pan
        return (Fixed::ONE, Fixed::ZERO);
    }

    let level_width_fixed = Fixed((*ddgame).level_width_sound << 16);
    let (lx, ly) = (*ddgame).listener_pos();

    distance_3d_attenuation(Fixed(lx), Fixed(ly), x, y, level_width_fixed, attenuation)
}

/// Record an active local sound in the tracking table — port of
/// RecordActiveSound (0x546260).
///
/// Finds a free slot in the 64-entry ring buffer (probing by incrementing
/// counter until channel_handle == 0), then writes the entry.
///
/// Convention: usercall(EAX=table, ESI=emitter) + stdcall(x, y, volume, channel_handle),
/// RET 0x10. Returns the counter value (used as handle with 0x40000000 bit).
unsafe fn record_active_sound(
    table: *mut ActiveSoundTable,
    emitter: *mut u8,
    x: i32,
    y: i32,
    volume: i32,
    channel_handle: i32,
) -> i32 {
    let t = &mut *table;

    // Probe for a free slot (channel_handle == 0)
    loop {
        t.counter = t.counter.wrapping_add(1);
        let slot = (t.counter & 0x3F) as usize;
        if t.entries[slot].channel_handle == 0 {
            break;
        }
    }

    let slot = (t.counter & 0x3F) as usize;
    let entry = &mut t.entries[slot];
    entry.pos_x = Fixed(x);
    entry.pos_y = Fixed(y);
    entry.emitter = emitter;
    entry.volume = Fixed(volume);
    entry.sequence = t.counter as i32;
    entry.channel_handle = channel_handle as u32;

    // Increment emitter ref count (emitter+0x08)
    if !emitter.is_null() {
        let ref_count = emitter.add(8) as *mut i32;
        *ref_count += 1;
    }

    t.counter as i32
}

/// Dispatch a local (positional) sound — port of DispatchLocalSound (0x546360).
///
/// Clamps volume, computes 3D distance attenuation and pan, calls
/// DSSound::play_sound_pooled, then records the active sound.
///
/// Convention: usercall(EAX=volume, EDI=active_sound_table) +
///   stdcall(sound_slot, flags, x, y, emitter), RET 0x14.
unsafe fn dispatch_local_sound(
    table: *mut ActiveSoundTable,
    volume: Fixed,
    sound_slot: SoundId,
    flags: i32,
    pos: (Fixed, Fixed),
    emitter: *mut u8,
) -> i32 {
    let volume = volume.min(Fixed::ONE);

    let ddgame = (*table).ddgame;
    let (volume_atten, pan) = compute_distance_params(ddgame, pos.0, pos.1);

    let scaled_volume = volume * volume_atten;

    // Call DSSound::play_sound_pooled (vtable slot 4)
    let sound = (*ddgame).sound;
    let sound_vt = &*(*sound).vtable;
    let handle =
        (sound_vt.play_sound_pooled)(sound, sound_slot, flags, Fixed::ONE, scaled_volume, pan);

    if handle == 0 {
        return 0;
    }

    record_active_sound(table, emitter, pos.0 .0, pos.1 .0, volume.0, handle)
}

/// Load and play a streaming sound — port of LoadAndPlayStreaming (0x546C20).
///
/// Checks suppression conditions, gets emitter position via vtable call,
/// then dispatches as a local sound. Returns handle | 0x40000000 on success,
/// 0 on failure, -1 (0xFFFFFFFF) if suppressed.
///
/// Replaces the former naked asm bridge `call_load_and_play_streaming`.
pub unsafe fn load_and_play_streaming(
    worm: *mut CTaskWorm,
    sound_id: SoundId,
    flags: u32,
    volume: Fixed,
) -> i32 {
    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let gi = &*(*ddgame).game_info;

    // Suppression checks (matching 0x546C20 exactly)
    if gi.sound_mute != 0 || (*ddgame).frame_counter < gi.sound_start_frame {
        return -1;
    }
    if (*ddgame).fast_forward_active != 0 {
        return -1;
    }

    let sound = (*ddgame).sound;
    if sound.is_null() {
        return 0;
    }

    // Get emitter position via sound_emitter vtable[0] (GetPosition)
    let task = worm as *mut CGameTask;
    let emitter = &(*task).sound_emitter;
    let mut pos_x: u32 = 0;
    let mut pos_y: u32 = 0;
    ((*emitter.vtable).get_position)(emitter, &mut pos_x, &mut pos_y);

    // Dispatch as local sound
    let table = (*ddgame).active_sounds;
    if table.is_null() {
        return 0;
    }

    let handle = dispatch_local_sound(
        table,
        volume,
        sound_id,
        flags as i32,
        (Fixed(pos_x as i32), Fixed(pos_y as i32)),
        emitter as *const _ as *mut u8,
    );
    if handle == 0 {
        return 0;
    }
    handle | 0x40000000
}

/// Load and play a streaming sound at a specific position — port of
/// LoadAndPlayStreamingPositional (0x546BB0).
///
/// Same as `load_and_play_streaming` but uses explicit (x, y) coordinates
/// instead of reading from the emitter. Used by teleport sound special case.
pub unsafe fn load_and_play_streaming_positional(
    worm: *mut CTaskWorm,
    sound_id: SoundId,
    flags: u32,
    volume: Fixed,
    x: Fixed,
    y: Fixed,
) -> i32 {
    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let gi = &*(*ddgame).game_info;

    // Suppression checks (matching 0x546BB0 exactly)
    if gi.sound_mute != 0 || (*ddgame).frame_counter < gi.sound_start_frame {
        return -1;
    }
    if (*ddgame).fast_forward_active != 0 {
        return -1;
    }

    let sound = (*ddgame).sound;
    if sound.is_null() {
        return 0;
    }

    let table = (*ddgame).active_sounds;
    if table.is_null() {
        return 0;
    }

    // Note: positional variant passes 0 for emitter (no ref tracking)
    let handle = dispatch_local_sound(
        table,
        volume,
        sound_id,
        flags as i32,
        (x, y),
        core::ptr::null_mut(),
    );
    if handle == 0 {
        return 0;
    }
    handle | 0x40000000
}
