//! MissileEntity::OnContact port (vtable slot 8, 0x00508C90).
//!
//! Called when a missile contacts another entity (terrain, worm, object).
//! Dispatches per missile type:
//! - **Standard / Cluster**: ricochet roll (decrement counter, RNG-gated X-mirror),
//!   else impact → explosion.
//! - **Homing**: consult target-lookup; on miss, zero velocity. Otherwise impact.
//! - **Sheep**: on first contact, stash pos/speed + arm re-entry countdown; on lock
//!   or side-match, terminate via slot-14.
//!
//! All paths reaching the generic "impact" tail play an impact sound, call
//! `WorldEntity::OnContact` (base impl) to transfer damage/forces to `other`, and
//! conditionally spawn explosions or fire-particle effects.
//!
//! Bridges out to WA.exe for: `WorldEntity::vt8`, `PlayImpactSound_Maybe`,
//! `MissileEntity::HomingTargetCheck_Maybe`, `MissileEntity::ImpactSpecialFx_Maybe`,
//! the `CreateExplosion` damage-jitter helper `FUN_00547CB0`, and the slot-14
//! terminator on self. `CreateExplosion` itself is the pure-Rust
//! `game::create_explosion`. RNG advances use the Rust `GameWorld::advance_rng`.

use openwa_core::fixed::Fixed;

use crate::entity::BaseEntity;
use crate::entity::missile::{MissileEntity, MissileType};
use crate::game::create_explosion::create_explosion;
use crate::rebase::rb;
use core::ffi::c_void;

// Ghidra VAs for the WA helpers we bridge to.
const VA_PLAY_IMPACT_SOUND: u32 = 0x004FF020;
const VA_HOMING_TARGET_CHECK: u32 = 0x005018F0;
const VA_CGAME_TASK_VT8: u32 = 0x004FFED0;
const VA_IMPACT_SPECIAL_FX: u32 = 0x00509BA0;
const VA_EXPLOSION_DAMAGE_JITTER: u32 = 0x00547CB0;

/// WorldEntity subclass-data offsets (inside `WorldEntity::subclass_data[0..0x54]`)
/// that MissileEntity::OnContact touches directly. These live in the base class's
/// opaque region rather than in `MissileEntity`'s own fields. The terminate-flag
/// field (+0x44) is written only by slot-14 dispatch, so we do not touch it
/// directly here.
const OFFSET_ACTION_FLAG: usize = 0x3C;
const OFFSET_SHEEP_STATE_FLAG: usize = 0x48;

/// Read/write a u32 field in the MissileEntity struct by absolute byte offset.
#[inline]
unsafe fn field_u32_mut(this: *mut MissileEntity, byte_offset: usize) -> *mut u32 {
    unsafe { (this as *mut u8).add(byte_offset) as *mut u32 }
}

/// `MissileEntity::OnContact` — pure-Rust port of 0x00508C90 (vtable slot 8).
///
/// Argument layout mirrors the original:
/// - `this` — missile.
/// - `other` — the entity we contacted (any BaseEntity subclass).
/// - `self_side_flags` — index (0..31) of the missile-local side/face that hit.
///   The caller (physics/collision) supplies this; `other->+0x30` supplies the
///   mirror-image face index on `other`'s side.
pub unsafe extern "thiscall" fn missile_on_contact(
    this: *mut MissileEntity,
    other: *mut BaseEntity,
    self_side_flags: u32,
) -> u32 {
    unsafe {
        // ---------------------------------------------------------------
        // 1. Accumulate |speed_x| + |speed_y| into the rolling half-speed
        //    accumulator at +0x124. This runs on every contact regardless of
        //    missile type, including the terminator path.
        // ---------------------------------------------------------------
        let speed_x = (*this).base.speed_x;
        let speed_y = (*this).base.speed_y;
        let abs_speed_sum = speed_x.0.unsigned_abs() + speed_y.0.unsigned_abs();
        *field_u32_mut(this, 0x124) =
            (*field_u32_mut(this, 0x124)).wrapping_add((abs_speed_sum >> 1) as u32);

        // Read other's contact face (low 5 bits used as a shift count via
        // `SHL reg, CL` which implicitly masks CL with 0x1F).
        let other_face_idx = BaseEntity::contact_face_slot_raw(other) & 0x1F;
        let other_face_bit = 1u32 << other_face_idx;

        let missile_type = (*this).missile_type;
        let contact_face_mask = (*this).contact_face_mask;

        // ---------------------------------------------------------------
        // 2. Sheep-specific pre-branch. Runs before the shared contact-
        //    disable/terminator checks so sheep can fall through to the
        //    terminator when the contact-face mask matches.
        // ---------------------------------------------------------------
        if missile_type == MissileType::Sheep {
            if (contact_face_mask & other_face_bit) != 0 || (*this).sheep_bailout_locked != 0 {
                terminator_bailout_stash(this, speed_x, speed_y);
                return 1;
            }
            if (*this).sheep_bailout_counter == 0 {
                (*this).sheep_stash_pos_x = (*this).base.pos_x;
                (*this).sheep_stash_pos_y = (*this).base.pos_y;
                (*this).sheep_stash_speed_x = (*this).base.speed_x;
                (*this).sheep_stash_speed_y = (*this).base.speed_y;
                *field_u32_mut(this, OFFSET_SHEEP_STATE_FLAG) = 1;
                (*this).sheep_action_flag = 0;
                (*this).sheep_bailout_counter = 10;
                return 1;
            }
            // Sheep past first contact falls through to shared contact logic.
        }

        // ---------------------------------------------------------------
        // 3. Shared contact-gate: reject via terminator if the contact-face
        //    mask matches or contact_phase is at the "disable" value (2).
        // ---------------------------------------------------------------
        if (contact_face_mask & other_face_bit) != 0 || (*this).contact_phase == 2 {
            terminator_bailout_stash(this, speed_x, speed_y);
            return 1;
        }

        // ---------------------------------------------------------------
        // 4. Homing early-out: homing_target_check returns 0 on no target →
        //    zero velocity and return. Convention: ECX=pos_x, EAX=pos_y+1,
        //    stack=[this].
        // ---------------------------------------------------------------
        if missile_type == MissileType::Homing {
            let pos_x = (*this).base.pos_x;
            let pos_y_plus_one = (*this).base.pos_y.wrapping_add(Fixed::ONE);
            let target_hit = homing_target_check(
                this as *mut c_void,
                pos_x,
                pos_y_plus_one,
                rb(VA_HOMING_TARGET_CHECK),
            );
            if target_hit == 0 {
                (*this).base.speed_x = Fixed::ZERO;
                (*this).base.speed_y = Fixed::ZERO;
                return 1;
            }
        }

        // ---------------------------------------------------------------
        // 5. Standard / Cluster ricochet: if self-side matches the mask,
        //    either terminate (counter exhausted) or RNG-roll to mirror X.
        // ---------------------------------------------------------------
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
                    *field_u32_mut(this, OFFSET_ACTION_FLAG) = 0;
                    MissileEntity::set_terminate_flag_raw(this, 1);
                    return 1;
                }
            }
            let world = {
                let this = this as *const BaseEntity;
                (*this).world
            };
            let rng = (*world).advance_rng();
            if ((rng & 0x3FF) % 100) < (*this).ricochet_chance_pct {
                // WA trick: NEG speed_x; if result signed (original speed_x > 0),
                // subtract |speed_y|, else add |speed_y|. speed_y is preserved.
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

        // ---------------------------------------------------------------
        // 6. Impact tail: impact sound + base WorldEntity::OnContact + optional
        //    explosion / fire-particle spawn.
        // ---------------------------------------------------------------
        let post_speed_x = (*this).base.speed_x.0;
        let post_speed_y = (*this).base.speed_y.0;
        let post_abs_sum = post_speed_x.unsigned_abs() + post_speed_y.unsigned_abs();
        // Scale by 0.4 (WA uses IMUL 0x66666667 + SAR 1 on the product).
        let impact_mag_scaled = (((post_abs_sum as u64) * 0x66666667u64) >> 33) as u32;

        if is_std_or_cluster && (*this).impact_sound_id != 0 {
            play_impact_sound(
                this as *mut c_void,
                (*this).impact_sound_id,
                impact_mag_scaled,
                rb(VA_PLAY_IMPACT_SOUND),
            );
        }

        // Base-class OnContact: thiscall(this, other, side_flags).
        cgameentity_on_contact_base(this, other, self_side_flags, rb(VA_CGAME_TASK_VT8));

        if is_std_or_cluster
            && (ricochet_side_mask & self_side_bit) != 0
            && (*this).explosion_damage != 0
        {
            let pos_x = (*this).base.pos_x;
            let pos_y = (*this).base.pos_y;
            if (*this).fire_particle_trigger == 0x40 {
                impact_special_fx(this as *mut c_void, pos_x, pos_y, rb(VA_IMPACT_SPECIAL_FX));
            }
            let damage = explosion_damage_jitter(
                (*this).explosion_damage,
                this as *mut c_void,
                (*this).explosion_damage_pct,
                rb(VA_EXPLOSION_DAMAGE_JITTER),
            );
            create_explosion(
                pos_x,
                pos_y,
                this as *mut BaseEntity,
                (*this).explosion_id,
                damage,
                0,
                (*this).spawn_params.owner_id,
            );
        }

        // ---------------------------------------------------------------
        // 7. Homing rebound: roll RNG; if low 4 nibbles are zero and contact
        //    side is left/right (2 or 4), set direction from sign of speed_x.
        // ---------------------------------------------------------------
        if missile_type == MissileType::Homing {
            let world = {
                let this = this as *const BaseEntity;
                (*this).world
            };
            let rng = (*world).advance_rng();
            if (rng & 0xF0000000) == 0 && (self_side_flags == 4 || self_side_flags == 2) {
                (*this).direction = if post_speed_x >= 0 { 1 } else { -1 };
            }
        }
        1
    }
}

/// Terminator bailout: clear action flag, invoke slot-14 on self, and stash
/// pre-terminator speed so downstream code (cluster spawn, splatter, etc.) can
/// read the missile's terminal velocity.
#[inline]
unsafe fn terminator_bailout_stash(this: *mut MissileEntity, speed_x: Fixed, speed_y: Fixed) {
    unsafe {
        *field_u32_mut(this, OFFSET_ACTION_FLAG) = 0;
        MissileEntity::set_terminate_flag_raw(this, 1);
        (*this).terminate_stash_speed_x = speed_x;
        (*this).terminate_stash_speed_y = speed_y;
    }
}

// ============================================================================
// WA.exe bridges — naked-asm trampolines for non-standard calling conventions.
// ============================================================================

/// Bridge: `PlayImpactSound_Maybe` (0x004FF020) — usercall: EDI = this,
/// stack = [sound_id, mag]. The target reads `[EDI+0xE0]` as the emitter
/// pointer and `[EDI+0x2C]` as the world pointer.
#[unsafe(naked)]
unsafe extern "C" fn play_impact_sound(_this: *mut c_void, _sound_id: u32, _mag: u32, _addr: u32) {
    core::arch::naked_asm!(
        "push ebx",
        "push edi",
        // Stack: 2 saves(8) + ret(4) = 12 to first arg
        "mov edi, [esp+12]", // this -> EDI
        "mov ebx, [esp+24]", // addr
        "push [esp+20]",     // mag
        "push [esp+20]",     // sound_id (shifted +4 after push above)
        "call ebx",
        "pop edi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: `WorldEntity::vt8` (0x004FFED0) — plain thiscall(this, other, side_flags).
#[unsafe(naked)]
unsafe extern "C" fn cgameentity_on_contact_base(
    _this: *mut MissileEntity,
    _other: *mut BaseEntity,
    _side_flags: u32,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov ecx, [esp+8]",  // this
        "mov ebx, [esp+20]", // addr
        "push [esp+16]",     // side_flags
        "push [esp+16]",     // other (shifted +4)
        "call ebx",
        "pop ebx",
        "ret",
    );
}

/// Bridge: `MissileEntity::HomingTargetCheck_Maybe` (0x005018F0) — usercall:
/// ECX = pos_x, EAX = pos_y + 1.0 (Fixed), stack = [this]. Returns EAX.
#[unsafe(naked)]
unsafe extern "C" fn homing_target_check(
    _this: *mut c_void,
    _pos_x: Fixed,
    _pos_y_plus_one: Fixed,
    _addr: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov ecx, [esp+12]", // pos_x
        "mov eax, [esp+16]", // pos_y_plus_one
        "mov ebx, [esp+20]", // addr
        "push [esp+8]",      // this
        "call ebx",
        "pop ebx",
        "ret",
    );
}

/// Bridge: `MissileEntity::ImpactSpecialFx_Maybe` (0x00509BA0) — usercall:
/// EDI = this, stack = [pos_x, pos_y].
#[unsafe(naked)]
unsafe extern "C" fn impact_special_fx(
    _this: *mut c_void,
    _pos_x: Fixed,
    _pos_y: Fixed,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push edi",
        // Stack: 2 saves(8) + ret(4) = 12 to first arg
        "mov edi, [esp+12]", // this
        "mov ebx, [esp+24]", // addr
        "push [esp+20]",     // pos_y
        "push [esp+20]",     // pos_x (shifted +4)
        "call ebx",
        "pop edi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: `FUN_00547CB0` (damage jitter) — usercall:
/// ESI = base damage, stack = [this, pct]. Returns EAX.
/// Function is `RET 0x8` (stdcall cleans 2 args).
#[unsafe(naked)]
unsafe extern "C" fn explosion_damage_jitter(
    _base_damage: u32,
    _this: *mut c_void,
    _pct: u32,
    _addr: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        // Stack: 2 saves(8) + ret(4) = 12 to first arg
        "mov esi, [esp+12]", // base_damage
        "mov ebx, [esp+24]", // addr
        "push [esp+20]",     // pct
        "push [esp+20]",     // this (shifted +4)
        "call ebx",
        "pop esi",
        "pop ebx",
        "ret",
    );
}
