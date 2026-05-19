//! `MissileEntity::OnContact` (0x00508C90, vtable slot 8). Pure-Rust port.

use openwa_core::fixed::Fixed;
use openwa_core::vec2::Vec2;

use crate::entity::BaseEntity;
use crate::entity::missile::{MissileEntity, MissileType};
use crate::entity::worm::WormEntity;
use crate::game::create_explosion::create_explosion;
use crate::generated::wa_calls;
use core::ffi::c_void;

#[inline]
unsafe fn field_u32_mut(this: *mut MissileEntity, byte_offset: usize) -> *mut u32 {
    unsafe { (this as *mut u8).add(byte_offset) as *mut u32 }
}

/// `MissileEntity::OnContact` (0x00508C90, vtable slot 8). `self_side_flags`
/// is the missile-local face index (0..31) supplied by the collision
/// dispatcher; `other.contact_face` holds the mirror face on `other`'s side.
pub unsafe extern "thiscall" fn missile_on_contact(
    this: *mut MissileEntity,
    other: *mut BaseEntity,
    self_side_flags: u32,
) -> u32 {
    unsafe {
        let speed_x = (*this).base.speed_x;
        let speed_y = (*this).base.speed_y;
        let abs_speed_sum = speed_x.0.unsigned_abs() + speed_y.0.unsigned_abs();
        *field_u32_mut(this, 0x124) =
            (*field_u32_mut(this, 0x124)).wrapping_add((abs_speed_sum >> 1) as u32);

        // Low 5 bits used as a shift count (`SHL reg, CL` masks CL with 0x1F).
        // `other` is always a WorldEntity subclass at the contact-dispatch
        // boundary even though the dispatcher passes it as BaseEntity.
        let other_face_idx = (*(other as *const crate::entity::WorldEntity)).contact_face & 0x1F;
        let other_face_bit = 1u32 << other_face_idx;

        let missile_type = (*this).missile_type;
        let contact_face_mask = (*this).contact_face_mask;

        if missile_type == MissileType::Digger {
            if (contact_face_mask & other_face_bit) != 0 || (*this).digger_bailout_locked != 0 {
                terminator_bailout_stash(this, speed_x, speed_y);
                return 1;
            }
            if (*this).digger_bailout_counter == 0 {
                (*this).digger_stash_pos = Vec2::new((*this).base.pos.x, (*this).base.pos.y);
                (*this).digger_stash_speed = Vec2::new((*this).base.speed_x, (*this).base.speed_y);
                (*this).base.subclass_data.digger_state_flag = 1;
                (*this).digger_action_flag = 0;
                (*this).digger_bailout_counter = 10;
                return 1;
            }
        }

        if (contact_face_mask & other_face_bit) != 0 || (*this).contact_phase == 2 {
            terminator_bailout_stash(this, speed_x, speed_y);
            return 1;
        }

        if missile_type == MissileType::Animal {
            let pos_x = (*this).base.pos.x;
            let pos_y_plus_one = (*this).base.pos.y.wrapping_add(Fixed::ONE);
            let target_hit = wa_calls::MissileEntity::HomingTargetCheck(
                pos_x,
                pos_y_plus_one,
                this as *mut c_void,
            );
            if target_hit == 0 {
                (*this).base.speed_x = Fixed::ZERO;
                (*this).base.speed_y = Fixed::ZERO;
                return 1;
            }
        }

        let self_side_bit = 1u32 << (self_side_flags & 0x1F);
        let is_std_or_cluster =
            matches!(missile_type, MissileType::Standard | MissileType::Cluster);
        let ricochet_side_mask = (*this).ricochet_side_mask;

        if is_std_or_cluster && (ricochet_side_mask & self_side_bit) != 0 {
            let counter = (*this).ricochet_counter;
            if counter != 0 {
                let new_counter = counter.wrapping_sub(1);
                (*this).ricochet_counter = new_counter;
                if (new_counter as i32) < 1 {
                    (*this).base.subclass_data.action_flag = 0;
                    MissileEntity::set_terminate_flag_raw(this, 1);
                    return 1;
                }
            }
            let world = (*(this as *const BaseEntity)).world;
            let rng = (*world).advance_rng();
            if ((rng & 0x3FF) % 100) < (*this).ricochet_chance_pct {
                // WA trick: NEG speed_x; if result negative (original speed_x > 0)
                // subtract |speed_y|, else add |speed_y|. speed_y preserved.
                let negated = speed_x.0.wrapping_neg();
                let abs_speed_y = speed_y.0.unsigned_abs() as i32;
                let new_speed_x = if negated < 0 {
                    negated.wrapping_sub(abs_speed_y)
                } else {
                    negated.wrapping_add(abs_speed_y)
                };
                (*this).base.speed_x = Fixed(new_speed_x);
            }
        }

        let post_speed_x = (*this).base.speed_x.0;
        let post_speed_y = (*this).base.speed_y.0;
        let post_abs_sum = post_speed_x.unsigned_abs() + post_speed_y.unsigned_abs();
        // Scale by 0.4 (WA: IMUL 0x66666667 + SAR 1 on the product).
        let impact_mag_scaled = (((post_abs_sum as u64) * 0x66666667u64) >> 33) as u32;

        if is_std_or_cluster && (*this).impact_sound_id != 0 {
            wa_calls::WormEntity::PlayImpactSound(
                this as *mut WormEntity,
                (*this).impact_sound_id,
                Fixed(impact_mag_scaled as i32),
            );
        }

        wa_calls::WorldEntity::vt8(this as *mut c_void, other as *mut c_void, self_side_flags);

        if is_std_or_cluster
            && (ricochet_side_mask & self_side_bit) != 0
            && (*this).explosion_damage != 0
        {
            let pos_x = (*this).base.pos.x;
            let pos_y = (*this).base.pos.y;
            if (*this).fire_particle_trigger == 0x40 {
                wa_calls::MissileEntity::ImpactSpecialFx(this as *mut c_void, pos_x, pos_y);
            }
            let damage = wa_calls::GameTask::calc_damage(
                (*this).explosion_damage as i32,
                this as *mut c_void,
                (*this).explosion_damage_pct as i32,
            );
            create_explosion(
                pos_x,
                pos_y,
                this as *mut BaseEntity,
                (*this).explosion_id,
                damage as u32,
                0,
                (*this).spawn_params.owner_id,
            );
        }

        if missile_type == MissileType::Animal {
            let world = (*(this as *const BaseEntity)).world;
            let rng = (*world).advance_rng();
            if (rng & 0xF0000000) == 0 && (self_side_flags == 4 || self_side_flags == 2) {
                (*this).direction = if post_speed_x >= 0 { 1 } else { -1 };
            }
        }
        1
    }
}

#[inline]
unsafe fn terminator_bailout_stash(this: *mut MissileEntity, speed_x: Fixed, speed_y: Fixed) {
    unsafe {
        (*this).base.subclass_data.action_flag = 0;
        MissileEntity::set_terminate_flag_raw(this, 1);
        (*this).terminate_stash_speed = Vec2::new(speed_x, speed_y);
    }
}
