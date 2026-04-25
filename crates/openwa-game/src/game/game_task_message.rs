//! `CGameTask::HandleMessage` port — vtable slot 2 at 0x004FF280.
//!
//! Three explicit cases, default broadcasts to children:
//! - **0x02 (FrameFinish)**: release the entity's sound-emitter handle if
//!   still active.
//! - **0x1c (Explosion)**: clamp health to ≥0, run the per-entity damage
//!   calc, accumulate into `damage_accum`, and on `caller_flag` report a
//!   damage percentage to `CTaskTurnGame` via msg `0x1d`.
//! - **0x4b**: clamp health to ≥0, then dispatch self vtable slot 17 with
//!   `(data[+0xC], data[+0x10], 0)`.
//! - default: `CTask::broadcast_message_raw`.
//!
//! Bridges out for the FPU-heavy `CGameTask::ComputeExplosionDamage` and the
//! two sound-emitter helpers; everything else is direct Rust.

use openwa_core::fixed::Fixed;

use crate::game::message::{ExplosionMessage, ExplosionReportMessage, TaskMessage};
use crate::rebase::rb;
use crate::task::{CGameTask, CTask, CTaskTurnGame};

/// Bridge: `CGameTask::ComputeExplosionDamage` (0x004FF390) — distance
/// falloff damage. Usercall: EDI = this (CGameTask), stack = [explosion_id,
/// damage, pos_x, pos_y]. Returns the actual damage applied to this entity.
const VA_COMPUTE_EXPLOSION_DAMAGE: u32 = 0x004FF390;
/// Bridge: `CGameTask::IsSoundHandleExpired` (0x00546CD0) — usercall
/// (ECX=this, EAX=handle); returns non-zero if the slot the handle refers
/// to has been reclaimed (sound finished or handle stale).
const VA_IS_SOUND_HANDLE_EXPIRED: u32 = 0x00546CD0;
/// Bridge: `CGameTask::ReleaseSoundHandle` (0x00546D20) — fastcall(ECX=this,
/// EDX=handle); stops + releases the entity's owned sound handle.
const VA_RELEASE_SOUND_HANDLE: u32 = 0x00546D20;

/// Byte offset on CGameTask of the entity's health (signed i32).
///
/// Lives inside `subclass_data`, which is genuinely polymorphic. Subclasses
/// that delegate damage handling to `CGameTask::HandleMessage` (mines, oil
/// drums, the gravestone CTaskCross, score bubbles, etc.) interpret this
/// slot as HP and rely on the clamp at the head of msg=0x1c / msg=0x4b.
/// Subclasses that override HandleMessage outright — most importantly
/// `CTaskWorm` — never reach this codepath; CTaskWorm reuses the same slot
/// as `set_action_field_raw` (an air-strike scratch flag), with worm HP
/// living at `CTaskWorm+0x178/0x17C` instead.
const OFFSET_HEALTH: usize = 0x48;

/// `CGameTask::HandleMessage` — pure-Rust port of 0x004FF280 (vtable slot 2).
pub unsafe extern "thiscall" fn cgametask_handle_message(
    this: *mut CGameTask,
    sender: *mut CTask,
    msg_type: TaskMessage,
    size: u32,
    data: *const u8,
) {
    unsafe {
        match msg_type {
            TaskMessage::FrameFinish => clear_owned_sound_handle(this),
            TaskMessage::Explosion => apply_explosion_damage(this, data),
            TaskMessage::SpecialImpact => dispatch_special_impact(this, data),
            _ => CTask::broadcast_message_raw(this as *mut CTask, sender, msg_type, size, data),
        }
    }
}

/// Clamp the entity's health to ≥0. Shared prologue of msg=0x1c and
/// msg=0x4b in WA's original.
#[inline]
unsafe fn clamp_health(this: *mut CGameTask) {
    unsafe {
        let hp = (this as *mut u8).add(OFFSET_HEALTH) as *mut i32;
        if *hp < 0 {
            *hp = 0;
        }
    }
}

/// FrameFinish (msg=0x02): if our owned sound has finished playing, ask the
/// sound subsystem to free the slot and forget the handle. Sounds that are
/// still playing keep their handle for the next frame's poll.
unsafe fn clear_owned_sound_handle(this: *mut CGameTask) {
    unsafe {
        let handle = (*this).sound_handle;
        if handle == 0 {
            return;
        }
        if is_sound_handle_expired(this, handle, rb(VA_IS_SOUND_HANDLE_EXPIRED)) == 0 {
            return;
        }
        release_sound_handle(this, handle, rb(VA_RELEASE_SOUND_HANDLE));
        (*this).sound_handle = 0;
    }
}

unsafe fn apply_explosion_damage(this: *mut CGameTask, data: *const u8) {
    unsafe {
        clamp_health(this);

        let msg = &*(data as *const ExplosionMessage);
        let dmg = compute_explosion_damage(
            this,
            msg.explosion_id,
            msg.damage,
            msg.pos_x,
            msg.pos_y,
            rb(VA_COMPUTE_EXPLOSION_DAMAGE),
        );

        (*this).damage_accum = (*this).damage_accum.wrapping_add(dmg);

        if msg.caller_flag == 0 {
            return;
        }

        // Report damage percentage to CTaskTurnGame for score / kill attribution.
        let damage_percent = (dmg as i32).wrapping_mul(100) / msg.damage as i32;
        let turn_game = CTaskTurnGame::from_shared_data(this as *const CTask);
        if turn_game.is_null() {
            return;
        }
        CTaskTurnGame::handle_typed_message_raw(
            turn_game,
            this,
            ExplosionReportMessage { damage_percent },
        );
    }
}

unsafe fn dispatch_special_impact(this: *mut CGameTask, data: *const u8) {
    unsafe {
        clamp_health(this);

        let arg1 = (data.add(0x0c) as *const u32).read();
        let arg2 = (data.add(0x10) as *const u32).read();

        // Vtable slot 17 (byte offset +0x44). thiscall(this, arg1, arg2, 0).
        // Subclasses install their own slot 17, so we dispatch through self.
        let vtable = *(this as *const *const usize);
        let slot17: unsafe extern "thiscall" fn(*mut CGameTask, u32, u32, u32) =
            core::mem::transmute(*vtable.add(17));
        slot17(this, arg1, arg2, 0);
    }
}

// ============================================================================
// WA.exe bridges — naked-asm trampolines for non-standard calling conventions.
// ============================================================================

/// Bridge: `CGameTask::ComputeExplosionDamage` — usercall(EDI=this,
/// stack=[explosion_id, damage, pos_x, pos_y]), RET 0x10. Returns the
/// actual damage applied to `this` after distance falloff.
#[unsafe(naked)]
unsafe extern "C" fn compute_explosion_damage(
    _this: *mut CGameTask,
    _explosion_id: u32,
    _damage: u32,
    _pos_x: Fixed,
    _pos_y: Fixed,
    _addr: u32,
) -> i32 {
    core::arch::naked_asm!(
        "push ebx",
        "push edi",
        // Stack: 2 saves(8) + ret(4) = 12 to first arg.
        "mov edi, [esp+12]", // this -> EDI
        "mov ebx, [esp+32]", // addr
        "push [esp+28]",     // pos_y
        "push [esp+28]",     // pos_x (shifted)
        "push [esp+28]",     // damage
        "push [esp+28]",     // explosion_id
        "call ebx",
        "pop edi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: `CGameTask::IsSoundHandleExpired` — usercall(ECX=this,
/// EAX=handle). Returns nonzero if the handle refers to a slot whose sound
/// has finished playing (so the owner can drop its reference).
#[unsafe(naked)]
unsafe extern "C" fn is_sound_handle_expired(
    _this: *mut CGameTask,
    _handle: u32,
    _addr: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg.
        "mov ecx, [esp+8]",  // this -> ECX
        "mov eax, [esp+12]", // handle -> EAX
        "mov ebx, [esp+16]", // addr
        "call ebx",
        "pop ebx",
        "ret",
    );
}

/// Bridge: `CGameTask::ReleaseSoundHandle` — thiscall(this, handle).
/// Stops the named handle and releases its slot.
#[unsafe(naked)]
unsafe extern "C" fn release_sound_handle(_this: *mut CGameTask, _handle: u32, _addr: u32) {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg.
        "mov ecx, [esp+8]",
        "mov edx, [esp+12]",
        "mov ebx, [esp+16]",
        "call ebx",
        "pop ebx",
        "ret",
    );
}
