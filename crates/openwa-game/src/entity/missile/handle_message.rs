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
use crate::entity::game_entity::WorldEntity;
use crate::game::game_entity_message::world_entity_handle_message;
use crate::game::message::{EntityMessage, ExplosionMessage};
use crate::rebase::rb;

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

// Rebased bridge addresses, initialized by [`init_addrs`].
//
// `Task_Missile::start_fuse_sound` (0x00508B50) — `__usercall(EDI = this,
// [stack] = sound_id)`, RET 4. Tries to start the fuse sound; if that
// fails, stashes -sound_id as a deferred-retry sentinel.
static mut START_FUSE_SOUND_ADDR: u32 = 0;
// `Task_Missile::start_dig_sound` (0x00508930) — same shape, slot 0x3E0.
static mut START_DIG_SOUND_ADDR: u32 = 0;
// `Task_Missile::start_super_animal` (0x0050AF40) — `__usercall(EAX = this)`,
// plain RET. Transitions a homing missile into super-animal control mode.
static mut START_SUPER_ANIMAL_ADDR: u32 = 0;
// `Task_Missile::finish_super_animal` (0x0050B020) — `__usercall(EAX = this)`,
// plain RET. Closes out super-animal mode (drains residual velocity into
// 1/3 carry-over and sets contact_phase = 2).
static mut FINISH_SUPER_ANIMAL_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        START_FUSE_SOUND_ADDR = rb(0x00508B50);
        START_DIG_SOUND_ADDR = rb(0x00508930);
        START_SUPER_ANIMAL_ADDR = rb(0x0050AF40);
        FINISH_SUPER_ANIMAL_ADDR = rb(0x0050B020);
    }
}

// ─── WA bridges ────────────────────────────────────────────────────────────

/// `__usercall(EDI = this, [stack] = sound_id)`, RET 4. EDI is callee-saved
/// per the x86 ABI, so the trampoline preserves it across the call.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_start_fuse_sound(_this: *mut MissileEntity, _sound_id: i32) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",   // this  (ret(4) + edi(4) = 8)
        "push dword ptr [esp+12]",      // sound_id (caller's stack arg)
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop edi",
        "ret 8",
        addr = sym START_FUSE_SOUND_ADDR,
    );
}

/// Same shape as [`bridge_start_fuse_sound`].
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_start_dig_sound(_this: *mut MissileEntity, _sound_id: i32) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",
        "push dword ptr [esp+12]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop edi",
        "ret 8",
        addr = sym START_DIG_SOUND_ADDR,
    );
}

/// `__usercall(EAX = this)`, plain RET. EAX is caller-saved.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_start_super_animal(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym START_SUPER_ANIMAL_ADDR,
    );
}

/// Same shape as [`bridge_start_super_animal`].
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_finish_super_animal(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym FINISH_SUPER_ANIMAL_ADDR,
    );
}

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

/// `0x1C` Explosion — forward inbound explosion broadcasts to the parent
/// `WorldEntity::HandleMessage` (which applies physics impulse / damage),
/// gated on:
///
/// - **Old/unforced path** (`game_version < 0x4E && _scheme_d99f == 0`):
///   forward only when [`explosion_response_flag`] is non-zero, payload
///   unchanged.
/// - **Modern/forced path** (otherwise): forward when either
///   [`explosion_response_flag`] is non-zero OR `_scheme_d99f != 0`,
///   first making a local copy of the [`ExplosionMessage`] with
///   [`caller_flag`] zeroed.
///
/// Always handled — the gate-failed paths drop the message silently in WA
/// (case body falls through to `break` → bottom canned-value return; no
/// parent dispatch).
///
/// Mirrors `MineEntity::HandleMessage`'s case 0x1C in shape but without
/// the alliance gate and settling-anim-flag side effects (those are
/// mine-specific).
///
/// [`explosion_response_flag`]: MissileEntity::explosion_response_flag
/// [`caller_flag`]: ExplosionMessage::caller_flag
unsafe fn msg_explosion(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let game_version = (*game_info).game_version;
        let scheme_d99f = (*game_info)._scheme_d99f;
        let responds = (*this).explosion_response_flag != 0;

        if game_version < 0x4E && scheme_d99f == 0 {
            if responds {
                world_entity_handle_message(
                    this as *mut WorldEntity,
                    sender,
                    EntityMessage::Explosion,
                    size,
                    data,
                );
            }
        } else if responds || scheme_d99f != 0 {
            // Modern path: copy the message and zero `caller_flag` before
            // forwarding. WA's actual copy is 0x408 bytes (presumed
            // tail-junk over-read inherited from a larger stack buffer);
            // the parent never reads past `ExplosionMessage`, so copying
            // just the typed struct is equivalent.
            let mut local = *(data as *const ExplosionMessage);
            local.caller_flag = 0;
            world_entity_handle_message(
                this as *mut WorldEntity,
                sender,
                EntityMessage::Explosion,
                size,
                &local as *const ExplosionMessage as *const u8,
            );
        }
    }
}

/// `0x2C` DetonateWeapon — manual detonate / state-cycle request from
/// the firing worm.
///
/// Gated on:
/// - the message originating from this missile's own owner
///   (`*data == spawn_params.owner_id`), AND
/// - the missile being above water (`_field_b0 == 0`).
///
/// When the gate passes:
/// 1. **Super-animal transition** — for homing missiles whose render data
///    enables super-animal control ([`super_animal_eligible`] != 0):
///     - `contact_phase == 0` → call `Task_Missile::start_super_animal`,
///       and return.
///     - `contact_phase == 1 && pos_y_int < world.water_kill_y` →
///       call `Task_Missile::finish_super_animal`, and return.
/// 2. **Detonate dispatch** — by [`detonate_response_mode`]:
///     - `1`: invoke vtable[14] (`set_terminate_flag`) with flag `2` if
///       `weapon_data[0x2D] == 3`, else flag `1`. The flag-`1` sub-branch
///       additionally sets [`_field_3d4`] = 1 when
///       `weapon_data[0x2D] == 1 && game_version < 0x1F0 &&
///       weapon_data[9] == 0x41`.
///     - `2`: zero `_render_data_0e_15[2]` (some companion flag) and
///       `detonate_response_mode`, then `fuse_timer = (rng & 0xFFFF) %
///       500`.
///
/// Always handled — the gate-failed paths are no-ops in WA.
///
/// [`super_animal_eligible`]: MissileEntity::super_animal_eligible
/// [`detonate_response_mode`]: MissileEntity::detonate_response_mode
/// [`_field_3d4`]: MissileEntity::_field_3d4
unsafe fn msg_detonate_weapon(this: *mut MissileEntity, data: *const u8) {
    unsafe {
        let sender_id = *(data as *const u32);
        if sender_id != (*this).spawn_params.owner_id {
            return;
        }
        if (*this).base._field_b0 != 0 {
            // Underwater — silently drop.
            return;
        }

        let world = (*(this as *const BaseEntity)).world;

        // Super-animal transition for eligible homing missiles.
        if matches!((*this).missile_type, super::MissileType::Homing)
            && (*this).super_animal_eligible != 0
        {
            match (*this).contact_phase {
                0 => {
                    bridge_start_super_animal(this);
                    return;
                }
                1 => {
                    let pos_y_int = (*this).base.pos_y.to_int();
                    if pos_y_int < (*world).water_kill_y {
                        bridge_finish_super_animal(this);
                        return;
                    }
                }
                _ => {}
            }
        }

        // Detonate response.
        match (*this).detonate_response_mode {
            1 => {
                let flag = if (*this).weapon_data[0x2D] == 3 { 2 } else { 1 };
                MissileEntity::set_terminate_flag_raw(this, flag);
                if flag == 1 {
                    let game_version = (*(*world).game_info).game_version;
                    if (*this).weapon_data[0x2D] == 1
                        && game_version < 0x1F0
                        && (*this).weapon_data[9] == 0x41
                    {
                        (*this)._field_3d4 = 1;
                    }
                }
            }
            2 => {
                (*this)._render_data_0e_15[2] = 0;
                (*this).detonate_response_mode = 0;
                let rng = (*world).advance_rng();
                (*this).fuse_timer = ((rng & 0xFFFF) % 500) as i32;
            }
            _ => {}
        }
    }
}

/// `0x7A` (122) — sound-handle restore. Sent on save/restore (and similar
/// sound-system reset events). Re-arms the missile's two sound slots when
/// they were previously stashed as `-sound_id` retry sentinels by a
/// failed `Task_Missile::start_*_sound` call.
///
/// Predicate (the inlined `Task_Missile::sub_508B90` / `sub_5088A0`): the
/// slot is a deferred retry iff `-slot` is non-negative, has bit `0x10000`
/// set, and the low 16 bits are `< 0x7F` (i.e. the original sound id was
/// `0x10000 ..= 0x1007E`, the music-style category).
///
/// Always handled — both sub-branches are conditional, and a no-op when
/// neither slot is in the retry state.
unsafe fn msg_sound_restore(this: *mut MissileEntity) {
    unsafe {
        let fuse_slot = (*this).fuse_sound_handle;
        if is_deferred_sound_retry(fuse_slot) {
            bridge_start_fuse_sound(this, fuse_slot.wrapping_neg());
        }
        let dig_slot = (*this).dig_sound_handle;
        if is_deferred_sound_retry(dig_slot) {
            bridge_start_dig_sound(this, dig_slot.wrapping_neg());
        }
    }
}

/// Inline port of `Task_Missile::sub_508B90` / `sub_5088A0` (12-instruction
/// predicates). Returns `true` when `slot` is a `-sound_id` retry sentinel
/// stashed by a previously-failed `start_*_sound` call.
#[inline]
fn is_deferred_sound_retry(slot: i32) -> bool {
    let neg = slot.wrapping_neg() as u32;
    (neg as i32) >= 0 && (neg & 0x10000) != 0 && (neg & 0xFFFE_FFFF) < 0x7F
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
            0x1C => {
                msg_explosion(this, sender, size, data);
                true
            }
            0x2C => {
                msg_detonate_weapon(this, data);
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
            0x7A => {
                msg_sound_restore(this);
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
