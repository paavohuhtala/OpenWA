//! `WorldEntity::HandleMessage` and its three downstream helpers. The
//! handler has three explicit cases; default broadcasts to children:
//! - **0x02 (FrameFinish)**: release the entity's owned sound handle if
//!   the slot it points at has finished playing.
//! - **0x1c (Explosion)**: clamp health to ≥0, run distance-falloff damage,
//!   accumulate, optionally report to `WorldRootEntity` for kill attribution.
//! - **0x4b (SpecialImpact)**: clamp health to ≥0, apply impulse via slot 17.
//! - default: `BaseEntity::broadcast_message_raw`.
//!
//! The three sound/damage helpers are also exposed for direct hooking so
//! WA's other callers (six for `IsSoundHandleExpired`, one for
//! `ReleaseSoundHandle`) land in our Rust ports too.

use openwa_core::fixed::Fixed;

use crate::audio::DSSound;
use crate::entity::{BaseEntity, WorldEntity, WorldRootEntity};
use crate::game::message::{
    EntityMessage, ExplosionMessage, ExplosionReportMessage, SpecialImpactMessage,
};

crate::define_addresses! {
    class "WorldEntity" {
        fn/Usercall WORLD_ENTITY_IS_SOUND_HANDLE_EXPIRED = 0x00546CD0;
        fn/Fastcall WORLD_ENTITY_RELEASE_SOUND_HANDLE = 0x00546D20;
        fn/Usercall WORLD_ENTITY_COMPUTE_EXPLOSION_DAMAGE = 0x004FF390;
    }
}

/// Health slot inside `subclass_data` — genuinely polymorphic.
/// Subclasses that delegate to `WorldEntity::HandleMessage` (mines, oil
/// drums, the gravestone CrossEntity, score bubbles, …) treat +0x48 as HP
/// and rely on the clamp at the head of msg=0x1c / msg=0x4b. Subclasses
/// that override HandleMessage outright — `WormEntity` — never reach this
/// path; WormEntity reuses +0x48 as an air-strike scratch flag, with HP at
/// `WormEntity+0x178/0x17C`.
const OFFSET_HEALTH: usize = 0x48;

/// `WorldEntity::HandleMessage` (0x004FF280, vtable slot 2).
pub unsafe extern "thiscall" fn world_entity_handle_message(
    this: *mut WorldEntity,
    sender: *mut BaseEntity,
    msg_type: EntityMessage,
    size: u32,
    data: *const u8,
) {
    unsafe {
        match msg_type {
            EntityMessage::FrameFinish => clear_owned_sound_handle(this),
            EntityMessage::Explosion => {
                apply_explosion_damage(this, data as *const ExplosionMessage)
            }
            EntityMessage::SpecialImpact => {
                dispatch_special_impact(this, data as *const SpecialImpactMessage)
            }
            _ => BaseEntity::broadcast_message_raw(
                this as *mut BaseEntity,
                sender,
                msg_type,
                size,
                data,
            ),
        }
    }
}

#[inline]
unsafe fn clamp_health(this: *mut WorldEntity) {
    unsafe {
        let hp = (this as *mut u8).add(OFFSET_HEALTH) as *mut i32;
        if *hp < 0 {
            *hp = 0;
        }
    }
}

unsafe fn clear_owned_sound_handle(this: *mut WorldEntity) {
    unsafe {
        let handle = (*this).sound_handle;
        if handle == 0 {
            return;
        }
        if sound_handle_expired(this, handle) == 0 {
            return;
        }
        release_sound_handle(this, handle);
        (*this).sound_handle = 0;
    }
}

/// `WorldEntity::IsSoundHandleExpired` (0x00546CD0).
///
/// Bit 30 (`0x40000000`) of the handle distinguishes positional/local sounds
/// (active-sound table, indexed by `handle & 0x3f`) from globally-mixed
/// sounds. Returns 0 when sound is disabled so the caller keeps the slot
/// parked indefinitely.
pub unsafe fn sound_handle_expired(this: *const WorldEntity, handle: u32) -> u32 {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let sound = (*world).sound;
        if sound.is_null() {
            return 0;
        }
        if (handle & 0x40000000) != 0 {
            let cleaned = handle & 0xbfffffff;
            let table = (*world).active_sounds;
            let entry = &(*table).entries[(cleaned & 0x3f) as usize];
            return (entry.sequence as u32 != cleaned || entry.channel_handle == 0) as u32;
        }
        DSSound::is_channel_finished_raw(sound, handle as i32) as u32
    }
}

/// `WorldEntity::ReleaseSoundHandle` (0x00546D20).
pub unsafe extern "fastcall" fn release_sound_handle(this: *mut WorldEntity, handle: u32) -> u32 {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let sound = (*world).sound;
        if sound.is_null() || (handle as i32) < 0 {
            return 0;
        }
        if (handle & 0x40000000) != 0 {
            let cleaned = (handle & 0xbfffffff) as i32;
            return (*(*world).active_sounds).stop_sound(cleaned) as u32;
        }
        DSSound::stop_channel_raw(sound, handle as i32)
    }
}

unsafe fn apply_explosion_damage(this: *mut WorldEntity, msg: *const ExplosionMessage) {
    unsafe {
        clamp_health(this);

        let dmg = compute_explosion_damage(
            this,
            (*msg).explosion_id,
            (*msg).damage,
            (*msg).pos_x,
            (*msg).pos_y,
        );

        (*this).damage_accum = (*this).damage_accum.wrapping_add(dmg);

        if (*msg).caller_flag == 0 {
            return;
        }

        let damage_percent = dmg.wrapping_mul(100) / (*msg).damage as i32;
        let world_root = WorldRootEntity::from_shared_data(this as *const BaseEntity);
        if world_root.is_null() {
            return;
        }
        WorldRootEntity::handle_typed_message_raw(
            world_root,
            this,
            ExplosionReportMessage { damage_percent },
        );
    }
}

/// `WorldEntity::ComputeExplosionDamage` (0x004FF390).
pub unsafe fn compute_explosion_damage(
    this: *mut WorldEntity,
    strength: u32,
    damage: u32,
    pos_x: Fixed,
    pos_y: Fixed,
) -> i32 {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let radius_factor = (*world)._field_5f0 as i32;
        let threshold = radius_factor
            .wrapping_mul(damage as i32)
            .wrapping_add(2)
            .wrapping_shl(17);
        if threshold == 0 {
            return 0;
        }

        let mut delta = [
            (*this).pos_x.0.wrapping_sub(pos_x.0),
            (*this).pos_y.0.wrapping_sub(pos_y.0),
        ];

        // VECTOR_NORMALIZE_{SIMPLE,OVERFLOW} chosen by game version in
        // InitGameState; both return the magnitude and write the unit
        // vector back into `delta`.
        let normalize: unsafe extern "stdcall" fn(*mut [i32; 2]) -> u32 =
            core::mem::transmute((*world).vector_normalize_fn);
        let mag = normalize(&mut delta) as i32;
        if mag > threshold {
            return 0;
        }

        let ratio = Fixed(threshold - mag) / Fixed(threshold);
        let scaled_dmg = ((damage as i32 as i64).wrapping_mul(ratio.0 as i64) >> 16) as i32;
        if scaled_dmg == 0 {
            return 0;
        }

        let recv_scale_raw = (*this)._field_cc.0;
        let knock_x = Fixed(compute_knockback_axis(
            scaled_dmg,
            delta[0],
            recv_scale_raw,
            strength as i32,
        ));
        let knock_y = Fixed(compute_knockback_axis(
            scaled_dmg,
            delta[1],
            recv_scale_raw,
            strength as i32,
        ));

        WorldEntity::add_impulse_raw(this, knock_x, knock_y, 0);

        scaled_dmg
    }
}

/// Wrapping 32-bit IMULs + signed `/5` and `/100` mirror MSVC's
/// strength-reduced sequence at 0x004FF40C..0x004FF483 — required for
/// bit-identical overflow behaviour with WA.
#[inline]
fn compute_knockback_axis(scaled_dmg: i32, delta: i32, recv_scale_raw: i32, strength: i32) -> i32 {
    let step1 = scaled_dmg.wrapping_mul(delta) / 5;
    let step2 = (((step1 as i64) * (recv_scale_raw as i64)) >> 16) as i32;
    step2.wrapping_mul(strength) / 100
}

unsafe fn dispatch_special_impact(this: *mut WorldEntity, msg: *const SpecialImpactMessage) {
    unsafe {
        clamp_health(this);
        let msg = &*msg;
        WorldEntity::add_impulse_raw(this, msg.impulse_x, msg.impulse_y, 0);
    }
}
