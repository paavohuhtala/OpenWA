//! Incremental port of `MissileEntity::HandleMessage` (0x0050B400, vtable
//! slot 2). The dispatcher handles a small set of bounded message cases
//! pure-Rust and falls through to WA's original for everything else.
//!
//! WA's HandleMessage contains a top-level early-bail
//! (`if (msg - 2 > 0x7C) return msg - 2`) and a per-case canned return
//! value indexed off `msg + 2`. The cases we port here all run to either
//! an early `return` or `break` in the original — neither path forwards
//! to the parent `WorldEntity::HandleMessage`. So a "handled" branch
//! simply suppresses fall-through; an "unhandled" branch defers to WA's
//! original via [`ORIGINAL_HANDLE_MESSAGE`].

use core::sync::atomic::{AtomicU32, Ordering};

use super::MissileEntity;
use crate::entity::base::BaseEntity;

type HandleMessageFn = unsafe extern "thiscall" fn(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
);

/// Saved original `MissileEntity::HandleMessage` (0x0050B400), populated
/// by `vtable_replace!` at install time.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

/// Reserved for future bridge-address registration. The current slice has
/// no WA bridges, but the shim exposes the hook so the install site
/// matches mine/oil_drum's pattern.
pub unsafe fn init_addrs() {}

/// HandleMessage selects between two "discriminator" slots inside
/// [`MissileEntity`]'s render-data block based on
/// [`is_cluster_pellet`](MissileEntity::is_cluster_pellet) — single-shot
/// missiles read [`_render_data_07`](MissileEntity::_render_data_07);
/// cluster pellets read [`_render_data_1a`](MissileEntity::_render_data_1a).
/// This corresponds to the `piVar8` view set up at the top of WA's
/// HandleMessage, and `animation_rate_kind() == piVar8[2]`.
#[inline]
unsafe fn animation_rate_kind(this: *const MissileEntity) -> u32 {
    unsafe {
        if (*this).is_cluster_pellet != 0 {
            (*this)._render_data_1a
        } else {
            (*this)._render_data_07
        }
    }
}

// ─── Per-case handlers ─────────────────────────────────────────────────────

/// `5` UpdateNonCritical — animation-phase update.
///
/// When the animation-rate kind discriminator is `3` and the missile is
/// either non-homing OR homing-but-flying-straight (not underwater /
/// not in super-animal control), the animation phase gets bumped by
/// `|speed_x| / 100 + 0xCCC` (mod 0x10000).
///
/// Always handled — the gate-failed path is a no-op in WA.
unsafe fn msg_update_non_critical(this: *mut MissileEntity) {
    unsafe {
        if animation_rate_kind(this) != 3 {
            return;
        }
        let homing = matches!((*this).missile_type, super::MissileType::Homing);
        let underwater = (*this).base._field_b0 != 0;
        let super_animal_active = (*this).contact_phase == 1;
        if homing && (underwater || super_animal_active) {
            return;
        }

        let abs_sx = (*this).base.speed_x.to_raw().wrapping_abs() as u32;
        let new = abs_sx
            .wrapping_div(100)
            .wrapping_add(0xCCC)
            .wrapping_add((*this).animation_phase)
            & 0xFFFF;
        (*this).animation_phase = new;
    }
}

/// `0x2D` MoveWeaponLeft / `0x2E` MoveWeaponRight — super-animal steering.
///
/// Only acts when the message sender's id matches `spawn_params.owner_id`
/// AND the missile is in super-animal control mode (contact_phase == 1).
/// The torque delta is `-0x5B0` for Left, `+0x5B0` for Right.
///
/// Old schemes (`game_version < 0x1D`): unconditionally adds delta to the
/// running [`super_animal_torque_accum`] accumulator.
///
/// New schemes: clamps the per-frame [`super_animal_torque_input`] to
/// `[-0x5B0, +0x5B0]`. The FrameFinish tick later folds the input into the
/// accumulator.
///
/// Always handled — the gate-failed path is a no-op in WA.
///
/// [`super_animal_torque_accum`]: MissileEntity::super_animal_torque_accum
/// [`super_animal_torque_input`]: MissileEntity::super_animal_torque_input
unsafe fn msg_move_weapon_dir(this: *mut MissileEntity, data: *const u8, delta: i32) {
    unsafe {
        let sender_id = *(data as *const u32);
        if sender_id != (*this).spawn_params.owner_id {
            return;
        }
        if (*this).contact_phase != 1 {
            return;
        }

        let world = (*(this as *const BaseEntity)).world;
        let game_version = (*(*world).game_info).game_version;

        if game_version < 0x1D {
            (*this).super_animal_torque_accum =
                (*this).super_animal_torque_accum.wrapping_add(delta as u32);
        } else {
            let candidate = (*this).super_animal_torque_input.wrapping_add(delta);
            (*this).super_animal_torque_input = candidate.clamp(-0x5B0, 0x5B0);
        }
    }
}

/// `0x7E` (126) — homing fuse-timer modifier sent by the homing-control UI.
///
/// `data` layout: `(sender_id: u32, mul: i32, div: i32)`. When the sender
/// owns this missile AND it's a homing missile (`missile_type == 3`):
/// - `mul >= 0` → `fuse_timer = fuse_timer + (fuse_timer * mul) / div`
/// - `mul < 0`  → `fuse_timer = i32::MAX` (effectively disable expiry)
///
/// Always handled — the gate-failed path is a no-op in WA.
///
/// NOTE: the multiplication is done as 32-bit (Ghidra-decomp interpretation);
/// on overflow the truncated u32 is reinterpreted as i32 before division.
/// If a desync surfaces here, WA's actual machine code may be doing 64-bit
/// IMUL/IDIV — revisit and switch to `i64` math.
unsafe fn msg_homing_fuse_modifier(this: *mut MissileEntity, data: *const u8) {
    unsafe {
        let sender_id = *(data as *const u32);
        if sender_id != (*this).spawn_params.owner_id {
            return;
        }
        if !matches!((*this).missile_type, super::MissileType::Homing) {
            return;
        }

        let mul = *(data.add(4) as *const i32);
        let div = *(data.add(8) as *const i32);
        let fuse = (*this).fuse_timer;

        if mul >= 0 {
            let product = (fuse as u32).wrapping_mul(mul as u32) as i32;
            let quotient = if div != 0 { product / div } else { 0 };
            (*this).fuse_timer = quotient.wrapping_add(fuse);
        } else {
            (*this).fuse_timer = i32::MAX;
        }
    }
}

// ─── Dispatcher ────────────────────────────────────────────────────────────

pub unsafe extern "thiscall" fn handle_message(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let handled = match msg_type {
            5 => {
                msg_update_non_critical(this);
                true
            }
            0x2D => {
                msg_move_weapon_dir(this, data, -0x5B0);
                true
            }
            0x2E => {
                msg_move_weapon_dir(this, data, 0x5B0);
                true
            }
            0x7E => {
                msg_homing_fuse_modifier(this, data);
                true
            }
            _ => false,
        };
        if !handled {
            fall_through(this, sender, msg_type, size, data);
        }
    }
}

unsafe fn fall_through(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    let raw = ORIGINAL_HANDLE_MESSAGE.load(Ordering::Relaxed);
    debug_assert!(
        raw != 0,
        "MissileEntity::HandleMessage original ptr not initialized; vtable_replace! ran?"
    );
    let f: HandleMessageFn = unsafe { core::mem::transmute(raw as usize) };
    unsafe { f(this, sender, msg_type, size, data) }
}
