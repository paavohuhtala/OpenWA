//! Pure-Rust ports of the missile dig/fuse sound bookkeeping helpers.
//!
//! Each missile owns two sound-handle slots — `fuse_sound_handle` (+0x3E4)
//! and `dig_sound_handle` (+0x3E0). Tri-state: `> 0` is an active id, `0`
//! is inactive, `< 0` is a `-sound_id` retry sentinel stashed when the
//! underlying `load_and_play_streaming` call returned `-1`. The next
//! `Unknown122` (sound-restore) message re-arms via the matching `start_*`.

use core::mem::offset_of;

use openwa_core::fixed::Fixed;

use super::MissileEntity;
use crate::audio::SoundId;
use crate::audio::sound_ops::load_and_play_streaming;
use crate::entity::game_entity::WorldEntity;
use crate::game::game_entity_message::{release_sound_handle, sound_handle_expired};

/// `-slot` is a deferred-retry sentinel iff non-negative, has bit `0x10000`
/// set, and the low 16 bits are `< 0x7F` (original sound id was in the
/// `0x10000..=0x1007E` music-style category). Inlined twice in WA
/// (`Task_Missile::sub_508B90` / `sub_5088A0`).
#[inline]
pub fn is_deferred_sound_retry(slot: i32) -> bool {
    let neg = slot.wrapping_neg() as u32;
    (neg as i32) >= 0 && (neg & 0x10000) != 0 && (neg & 0xFFFE_FFFF) < 0x7F
}

// ─── start ─────────────────────────────────────────────────────────────────

/// `Task_Missile::start_fuse_sound` (0x00508B50).
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

/// `Task_Missile::start_dig_sound` (0x00508930).
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

/// `Task_Missile::stop_fuse_sound` (0x00508C10).
pub unsafe fn stop_fuse_sound(this: *mut MissileEntity) {
    unsafe { stop_handle(this, offset_of!(MissileEntity, fuse_sound_handle)) }
}

/// `Task_Missile::stop_dig_sound` (0x00508970).
pub unsafe fn stop_dig_sound(this: *mut MissileEntity) {
    unsafe { stop_handle(this, offset_of!(MissileEntity, dig_sound_handle)) }
}

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

/// `Task_Missile::check_fuse_sound` (0x00508BC0).
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

/// `Task_Missile::check_dig_sound` (0x005088D0).
pub unsafe fn check_dig_sound(this: *mut MissileEntity) {
    unsafe {
        let slot = (*this).dig_sound_handle;
        if is_deferred_sound_retry(slot) {
            return;
        }

        if (*this).digger_bailout_counter != 0 {
            if slot == 0 {
                let sound_id = (*this)._render_data_1a as i32;
                start_dig_sound(this, sound_id);
            }
            if (*this).digger_bailout_counter != 0 {
                return;
            }
        }
        if (*this).dig_sound_handle != 0 {
            stop_dig_sound(this);
        }
    }
}
