//! Pure-Rust ports of `Task_Missile::start_super_animal` (0x0050AF40) and
//! `Task_Missile::finish_super_animal` (0x0050B020) — the symmetric
//! transitions into and out of super-animal jetpack steering.

use openwa_core::fixed::Fixed;

use super::{MissileEntity, sound};
use crate::audio::SoundId;
use crate::audio::sound_ops::{load_and_play_streaming, play_sound_local};
use crate::entity::Entity;
use crate::entity::game_entity::WorldEntity;
use crate::game::message::{
    Unknown123Message, WeaponClaimControlMessage, WeaponReleaseControlMessage,
};

/// `Task_Missile::start_super_animal` (0x0050AF40) — transitions an
/// `MissileType::Animal` projectile into super-animal (jetpack-steered) mode.
pub(super) unsafe fn start_super_animal(this: *mut MissileEntity) {
    unsafe {
        (*this).contact_phase = 1;
        (*this).base.subclass_data.digger_state_flag = 1;
        (*this).base.speed_x = Fixed::ZERO;
        (*this).base.speed_y = Fixed::ZERO;
        (*this).super_animal_torque_accum = Fixed::ZERO;
        (*this).animation_phase = 0;

        let owner_id = (*this).spawn_params.owner_id;
        if owner_id != 0 {
            (*this).broadcast_via_world_root(WeaponClaimControlMessage {
                team_index: owner_id,
            });
        }

        let start_sound = (*this).super_animal_start_sound_id;
        if start_sound != 0 {
            play_sound_local(
                this as *mut WorldEntity,
                SoundId(start_sound),
                5,
                Fixed::ONE,
                Fixed::ONE,
            );
        }

        let loop_sound = (*this).super_animal_loop_sound_id as i32;
        if loop_sound != 0 {
            sound::stop_fuse_sound(this);
            let handle = load_and_play_streaming(
                this as *mut WorldEntity,
                SoundId(loop_sound as u32),
                4,
                Fixed::ONE,
            );
            (*this).fuse_sound_handle = if handle == -1 {
                // Suppressed — stash `-sound_id` retry sentinel.
                loop_sound.wrapping_neg()
            } else {
                handle
            };
        }

        // bucket_mask = render_timer | contact_face_mask
        (*this).base.bucket_mask = (*this).render_timer as u32 | (*this).contact_face_mask;
    }
}

/// `Task_Missile::finish_super_animal` (0x0050B020) — exits super-animal mode
/// and returns the missile to standard ballistic motion (with retained but
/// damped velocity).
pub(super) unsafe fn finish_super_animal(this: *mut MissileEntity) {
    unsafe {
        // Damp velocity by 1/3. WA uses the IMUL 0x55555556 magic constant;
        // Rust's `i32 / 3` matches the C round-toward-zero semantics.
        let sx = (*this).base.speed_x.to_raw();
        let sy = (*this).base.speed_y.to_raw();
        (*this).base.speed_x = Fixed::from_raw(sx / 3);
        (*this).base.speed_y = Fixed::from_raw(sy / 3);

        (*this).contact_phase = 2;
        (*this).base.subclass_data.digger_state_flag = 0;

        // Direction sign from low 16 bits of torque accumulator.
        let torque_low = (*this).super_animal_torque_accum.to_raw() as u32 & 0xFFFF;
        (*this).direction = if torque_low < 0x8000 { 1 } else { -1 };

        let owner_id = (*this).spawn_params.owner_id;
        if owner_id != 0 {
            (*this).broadcast_via_world_root(WeaponReleaseControlMessage {
                team_index: owner_id,
            });
            (*this).broadcast_via_world_root(Unknown123Message {
                team_index: owner_id,
            });
        }

        if (*this).super_animal_loop_sound_id != 0 {
            sound::stop_fuse_sound(this);
        }

        // bucket_mask = sprite_size | contact_face_mask
        (*this).base.bucket_mask = (*this).sprite_size.to_raw() as u32 | (*this).contact_face_mask;
    }
}
