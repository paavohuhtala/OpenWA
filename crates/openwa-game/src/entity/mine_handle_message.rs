//! Incremental port of `MineEntity::HandleMessage` (0x005072E0, vtable slot 2).
//!
//! Five explicit cases plus a default. All but `0x02 Tick` are dispatched
//! here; `Tick` falls through to the original WA function (saved into
//! [`ORIGINAL_HANDLE_MESSAGE`] by `vtable_replace!`).

use core::sync::atomic::{AtomicU32, Ordering};

use super::base::BaseEntity;
use super::game_entity::WorldEntity;
use super::mine::MineEntity;
use crate::game::game_entity_message::{alliance_blocks_damage, world_entity_handle_message};
use crate::game::message::{EntityMessage, ExplosionMessage, SpecialImpactMessage};
use crate::rebase::rb;

/// Subclass-data offset of MineEntity's "anim flag" slot (mine offset 0x74,
/// inside `WorldEntity::subclass_data` which starts at 0x30 → index 0x44).
/// Written by case 0x1C / 0x4B when the mine is still settling and the
/// scheme is new enough; meaning is otherwise opaque.
const SUBCLASS_OFFSET_ANIM_FLAG: usize = 0x44;

type HandleMessageFn = unsafe extern "thiscall" fn(
    this: *mut MineEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
);

/// Saved original `MineEntity::HandleMessage` (0x005072E0), populated by
/// `vtable_replace!` at install time.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

// Rebased helper addresses, initialized by `init_addrs()`.
//
// `StepRopePhysics_Maybe` (0x005003D0) — usercall(stdcall this on stack,
// AL = mode), RET 0x4. Same function `WormEntity::HandleMessage` case 0x3
// calls; the name is misleading — it operates on any WorldEntity subclass.
// AL=0 runs the full step.
static mut MINE_STEP_ROPE_PHYSICS_ADDR: u32 = 0;
// `Task_Mine__render` (0x00506EF0) — stdcall(this), RET 0x4. Mine's
// per-frame draw routine (sprite + arming-light + countdown text).
static mut MINE_RENDER_ADDR: u32 = 0;
// 0x00500630 — usercall(EAX = this), no stack args, plain RET. Tail
// companion to `StepRopePhysics_Maybe`. Was previously guessed as
// "RestoreKamikazeState" — that name was wrong.
static mut MINE_ROPE_PHYSICS_TAIL_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        MINE_STEP_ROPE_PHYSICS_ADDR = rb(0x005003D0);
        MINE_RENDER_ADDR = rb(0x00506EF0);
        MINE_ROPE_PHYSICS_TAIL_ADDR = rb(0x00500630);
    }
}

/// `__usercall(stdcall this on stack, AL = mode)`, RET 0x4. Bridge zeroes
/// AL explicitly before the call, matching WA's `XOR AL,AL` at the case-0x3
/// call site.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_step_rope_physics(_this: *mut MineEntity) {
    core::arch::naked_asm!(
        "xor al, al",
        "push dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym MINE_STEP_ROPE_PHYSICS_ADDR,
    );
}

/// Plain stdcall(this), RET 0x4.
#[inline]
unsafe fn bridge_mine_render(this: *mut MineEntity) {
    type Fn = unsafe extern "stdcall" fn(*mut MineEntity);
    let f: Fn = unsafe { core::mem::transmute(MINE_RENDER_ADDR as usize) };
    unsafe { f(this) }
}

/// `__usercall(EAX = this)`, no stack args, plain RET. Tail companion to
/// `bridge_step_rope_physics` in case 0x3.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_rope_physics_tail(_this: *mut MineEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym MINE_ROPE_PHYSICS_TAIL_ADDR,
    );
}

/// Read MineEntity's settle/arm-delay timer at offset 0x11C.
#[inline]
unsafe fn arm_delay(this: *const MineEntity) -> i32 {
    unsafe { (*this)._unknown_11c as i32 }
}

/// Anim-flag write performed by both case 0x1C and case 0x4B when the mine
/// is still settling on a modern-enough scheme.
#[inline]
unsafe fn maybe_set_settling_anim_flag(this: *mut MineEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        if (*game_info).game_version > 0x3C && arm_delay(this) > 0 {
            let dst = core::ptr::addr_of_mut!((*this).base.subclass_data[SUBCLASS_OFFSET_ANIM_FLAG])
                as *mut u32;
            *dst = (*game_info)._field_d780;
        }
    }
}

/// `0x15 GameOver` — clear the trigger-armed flag and the triggered
/// latch, leaving the mine inert. Despite the enum name this fires at
/// round end, not match end.
unsafe fn msg_game_round_end(this: *mut MineEntity) {
    unsafe {
        (*this)._field_104 = 0;
        (*this)._field_128 = 0;
    }
}

/// `0x03 RenderScene` — parent dispatch, then run the rope-physics step,
/// the per-frame draw, and the rope-physics tail.
unsafe fn msg_render(this: *mut MineEntity, sender: *mut BaseEntity, size: u32, data: *const u8) {
    unsafe {
        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::RenderScene,
            size,
            data,
        );
        bridge_step_rope_physics(this);
        bridge_mine_render(this);
        bridge_rope_physics_tail(this);
    }
}

/// `0x1C Explosion` — alliance gate, settling-anim-flag write, then mutate
/// `caller_flag` to 0 and forward to parent. Old (game_version ≤ 0x4D) and
/// new schemes differ in whether the mutation lands on the original
/// message buffer (in-place) or on a local copy. Both paths suppress the
/// `WorldRoot` kill-attribution report that the parent would otherwise
/// emit for this mine.
unsafe fn msg_explosion(
    this: *mut MineEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let msg = data as *const ExplosionMessage;
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;

        let owner_id = (*msg).owner_id;
        let placer_team = (*this).placer_team_index;
        if owner_id != 0
            && placer_team != 0
            && alliance_blocks_damage(world, owner_id, placer_team as u32)
        {
            return;
        }

        maybe_set_settling_anim_flag(this);

        let game_version = (*game_info).game_version;
        if game_version > 0x4D && (*msg).caller_flag != 0 {
            // Modern path: copy the message, zero the copy's caller_flag,
            // forward the copy. WA copies 0x408 bytes; only the first 0x1C
            // are populated (`ExplosionMessage`) — the parent never reads
            // past that, so the overlong copy is wasted work we don't
            // need to reproduce.
            let mut local = *msg;
            local.caller_flag = 0;
            world_entity_handle_message(
                this as *mut WorldEntity,
                sender,
                EntityMessage::Explosion,
                size,
                &local as *const ExplosionMessage as *const u8,
            );
        } else {
            // Legacy path: mutate caller_flag in place on the original
            // buffer, then forward.
            (*(msg as *mut ExplosionMessage)).caller_flag = 0;
            world_entity_handle_message(
                this as *mut WorldEntity,
                sender,
                EntityMessage::Explosion,
                size,
                data,
            );
        }
    }
}

/// `0x4B SpecialImpact` — alliance gate, settling-anim-flag write, then
/// forward to parent unchanged.
unsafe fn msg_special_impact(
    this: *mut MineEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let msg = data as *const SpecialImpactMessage;
        let world = (*(this as *const BaseEntity)).world;

        let source_team = (*msg).source_team_index;
        let placer_team = (*this).placer_team_index;
        if source_team != 0
            && placer_team != 0
            && alliance_blocks_damage(world, source_team, placer_team as u32)
        {
            return;
        }

        maybe_set_settling_anim_flag(this);

        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::SpecialImpact,
            size,
            data,
        );
    }
}

pub unsafe extern "thiscall" fn handle_message(
    this: *mut MineEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let Ok(msg) = EntityMessage::try_from(msg_type) else {
            // Unparseable message — fall through to WA's original so its
            // own default handler (parent dispatch) sees the raw u32.
            return fall_through(this, sender, msg_type, size, data);
        };

        match msg {
            // Mine's case 0x02 covers both `FrameFinish` (parent's
            // sound-handle release) AND the per-frame tick body — the
            // tick block isn't a separate case in WA, it sits past the
            // switch's `break`. Stays in WA until slice m2.
            EntityMessage::FrameFinish => fall_through(this, sender, msg_type, size, data),
            EntityMessage::RenderScene => msg_render(this, sender, size, data),
            EntityMessage::GameOver => msg_game_round_end(this),
            EntityMessage::Explosion => msg_explosion(this, sender, size, data),
            EntityMessage::SpecialImpact => msg_special_impact(this, sender, size, data),
            other => {
                world_entity_handle_message(this as *mut WorldEntity, sender, other, size, data)
            }
        }
    }
}

unsafe fn fall_through(
    this: *mut MineEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    let raw = ORIGINAL_HANDLE_MESSAGE.load(Ordering::Relaxed);
    debug_assert!(
        raw != 0,
        "MineEntity::HandleMessage original ptr not initialized; vtable_replace! ran?"
    );
    let f: HandleMessageFn = unsafe { core::mem::transmute(raw as usize) };
    unsafe { f(this, sender, msg_type, size, data) }
}
