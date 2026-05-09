//! Pure-Rust ports of the missile dig/fuse sound bookkeeping helpers.
//!
//! Each missile owns two sound-handle slots — [`fuse_sound_handle`] (+0x3E4)
//! and [`dig_sound_handle`] (+0x3E0) — that are tri-state:
//! - `> 0`  : an active GameWorld active-sound entry id;
//! - `0`    : inactive;
//! - `< 0`  : a `-sound_id` retry sentinel stashed when the underlying
//!   [`load_and_play_streaming`] call returned `-1` (sound system busy /
//!   muted). On the next `Unknown122` (sound-restore) message the slot
//!   is re-armed via the matching `start_*` helper.
//!
//! Six leaf functions are ported here:
//! - [`start_fuse_sound`] (0x00508B50) / [`start_dig_sound`] (0x00508930).
//! - [`stop_fuse_sound`]  (0x00508C10) / [`stop_dig_sound`]  (0x00508970).
//! - [`check_fuse_sound`] (0x00508BC0) / [`check_dig_sound`] (0x005088D0).
//!
//! [`fuse_sound_handle`]: MissileEntity::fuse_sound_handle
//! [`dig_sound_handle`]: MissileEntity::dig_sound_handle
//! [`load_and_play_streaming`]: crate::audio::sound_ops::load_and_play_streaming

use core::mem::offset_of;

use openwa_core::fixed::Fixed;

use super::MissileEntity;
use crate::audio::SoundId;
use crate::audio::sound_ops::load_and_play_streaming;
use crate::entity::game_entity::WorldEntity;
use crate::game::game_entity_message::{release_sound_handle, sound_handle_expired};

/// Predicate for the `-sound_id` deferred-retry sentinel encoding.
///
/// Inlined twice in WA (`Task_Missile::sub_508B90` / `sub_5088A0`); the
/// `unsafe extern "C" fn`-style reading: a slot is a retry sentinel iff
/// `-slot >= 0 && (-slot & 0x10000) != 0 && (-slot & 0xFFFEFFFF) < 0x7F`.
#[inline]
pub fn is_deferred_sound_retry(slot: i32) -> bool {
    let neg = slot.wrapping_neg() as u32;
    (neg as i32) >= 0 && (neg & 0x10000) != 0 && (neg & 0xFFFE_FFFF) < 0x7F
}

// ─── start ─────────────────────────────────────────────────────────────────

/// `Task_Missile::start_fuse_sound` (0x00508B50). Stops the prior fuse
/// sound (if any), starts a new streaming sound on channel 4, and stashes
/// either the live handle or `-sound_id` (busy-retry sentinel) in
/// [`fuse_sound_handle`](MissileEntity::fuse_sound_handle).
pub unsafe fn start_fuse_sound(this: *mut MissileEntity, sound_id: i32) {
    unsafe {
        stop_handle(this, offset_of!(MissileEntity, fuse_sound_handle));
        let handle = load_and_play_streaming(
            this as *mut WorldEntity,
            SoundId(sound_id as u32),
            4,
            Fixed::ONE,
        );
        (*this).fuse_sound_handle = handle;
        if handle == -1 {
            (*this).fuse_sound_handle = sound_id.wrapping_neg();
        }
    }
}

/// `Task_Missile::start_dig_sound` (0x00508930). Same shape as
/// [`start_fuse_sound`], targeting [`dig_sound_handle`].
///
/// [`dig_sound_handle`]: MissileEntity::dig_sound_handle
pub unsafe fn start_dig_sound(this: *mut MissileEntity, sound_id: i32) {
    unsafe {
        stop_handle(this, offset_of!(MissileEntity, dig_sound_handle));
        let handle = load_and_play_streaming(
            this as *mut WorldEntity,
            SoundId(sound_id as u32),
            4,
            Fixed::ONE,
        );
        (*this).dig_sound_handle = handle;
        if handle == -1 {
            (*this).dig_sound_handle = sound_id.wrapping_neg();
        }
    }
}

// ─── stop ──────────────────────────────────────────────────────────────────

/// `Task_Missile::stop_fuse_sound` (0x00508C10). Releases the active
/// fuse-sound handle (no-op if the slot is `0` or holds a retry sentinel)
/// and clears the slot to `0`.
pub unsafe fn stop_fuse_sound(this: *mut MissileEntity) {
    unsafe { stop_handle(this, offset_of!(MissileEntity, fuse_sound_handle)) }
}

/// `Task_Missile::stop_dig_sound` (0x00508970). Same shape as
/// [`stop_fuse_sound`], targeting [`dig_sound_handle`].
///
/// [`dig_sound_handle`]: MissileEntity::dig_sound_handle
pub unsafe fn stop_dig_sound(this: *mut MissileEntity) {
    unsafe { stop_handle(this, offset_of!(MissileEntity, dig_sound_handle)) }
}

/// Shared body for the two stop helpers. Mirrors WA's
/// `Task_Missile::stop_*_sound` — on a non-zero / non-negative handle,
/// releases via [`release_sound_handle`] and zeroes the slot.
#[inline]
unsafe fn stop_handle(this: *mut MissileEntity, slot_offset: usize) {
    unsafe {
        let slot = (this as *mut u8).add(slot_offset) as *mut i32;
        let handle = *slot;
        if handle == 0 {
            return;
        }
        if (handle as i32) >= 0 {
            release_sound_handle(this as *mut WorldEntity, handle as u32);
        }
        *slot = 0;
    }
}

// ─── check ─────────────────────────────────────────────────────────────────

/// `Task_Missile::check_fuse_sound` (0x00508BC0). Per-frame poller called
/// from the FrameFinish tick: if the fuse-sound handle is live (not a
/// retry sentinel) and the underlying channel has finished playing,
/// release it and clear the slot.
pub unsafe fn check_fuse_sound(this: *mut MissileEntity) {
    unsafe {
        let slot = (*this).fuse_sound_handle;
        if !is_deferred_sound_retry(slot)
            && slot != 0
            && sound_handle_expired(this as *mut WorldEntity, slot as u32) != 0
        {
            release_sound_handle(this as *mut WorldEntity, slot as u32);
            (*this).fuse_sound_handle = 0;
        }
    }
}

/// `Task_Missile::check_dig_sound` (0x005088D0). Per-frame poller for the
/// dig-sound slot: if the missile is still buried
/// ([`sheep_bailout_counter`] != 0) and the slot is empty, re-arm via
/// [`start_dig_sound`]; otherwise stop the sound when no longer buried.
///
/// The retry-sentinel guard at the top mirrors [`check_fuse_sound`] —
/// a sentinel bypasses both the start *and* the stop branches so the
/// next [`Unknown122`] re-arm gets a clean slate.
///
/// [`sheep_bailout_counter`]: MissileEntity::sheep_bailout_counter
/// [`Unknown122`]: crate::game::message::EntityMessage::Unknown122
pub unsafe fn check_dig_sound(this: *mut MissileEntity) {
    unsafe {
        let slot = (*this).dig_sound_handle;
        if is_deferred_sound_retry(slot) {
            return;
        }

        if (*this).sheep_bailout_counter != 0 {
            if slot == 0 {
                let sound_id = (*this)._render_data_1a as i32;
                start_dig_sound(this, sound_id);
            }
            if (*this).sheep_bailout_counter != 0 {
                return;
            }
        }
        if (*this).dig_sound_handle != 0 {
            stop_dig_sound(this);
        }
    }
}
