//! Full port of `OilDrumEntity::HandleMessage` (0x005050B0, vtable slot 2).
//!
//! Covers every dispatch slot of WA's `(msg_type - 2) << 2` jumptable:
//! - case 0x02 (FrameFinish) — per-frame tick + off-bottom drop /
//!   detonate-then-free.
//! - case 0x03 (RenderScene) — rope-physics step / [`super::render::oil_drum_render`] /
//!   rope-physics tail / parent dispatch.
//! - case 0x1C (Explosion) — alliance-aware damage forward + detonate gate.
//! - case 0x4B (SpecialImpact) — direct detonate (kind=non-real) or
//!   threshold-gated detonate (kind=real).
//! - default (any other id in `[0x02..=0x4B]` not enumerated above, plus
//!   `< 0x02 || > 0x4B`) — straight forward to `WorldEntity::HandleMessage`.
//!
//! Also hosts:
//!  * [`free`] — `OilDrumEntity::Free` (vtable slot 1) port.
//!  * [`detonate`] — `Task_OilDrum::detonate` (0x00504DF0) port. Spawns
//!    the explosion + 4 corner FireEntity instances + 0x3B sound.

use core::sync::atomic::{AtomicU32, Ordering};

use super::{OilDrumEntity, OilDrumEntityVtable};
use crate::audio::{SoundId, sound_ops::play_sound_local};
use crate::engine::EntityActivityQueue;
use crate::engine::world::GameWorld;
use crate::entity::base::{BaseEntity, SharedDataTable};
use crate::entity::fire::{FireEntity, FireEntityInit, fire_entity_construct};
use crate::entity::game_entity::WorldEntity;
use crate::game::create_explosion::create_explosion;
use crate::game::game_entity_message::world_entity_handle_message;
use crate::game::message::{EntityMessage, ExplosionMessage, SpecialImpactMessage};
use crate::rebase::rb;
use crate::wa_alloc::{wa_free, wa_malloc};
use openwa_core::fixed::Fixed;

type HandleMessageFn = unsafe extern "thiscall" fn(
    this: *mut OilDrumEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
);

/// Saved original `OilDrumEntity::HandleMessage` (0x005050B0), populated
/// by `vtable_replace!` at install time.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

// Rebased helper addresses, initialised by [`init_addrs`].
//
// `StepRopePhysics_Maybe` (0x005003D0) — usercall(stdcall this on stack,
// AL = mode), RET 0x4. Same function MineEntity / WormEntity case 0x03
// invokes. AL=0 runs the full step.
static mut OIL_STEP_ROPE_PHYSICS_ADDR: u32 = 0;
// Tail companion at 0x00500630 — usercall(EAX = this), no stack args.
static mut OIL_ROPE_PHYSICS_TAIL_ADDR: u32 = 0;

// Tick-body bridges:
// `EntityActivityQueue::ResetRank` (0x00541790) — usercall(EAX=queue,
// [stack]=slot), RET 0x4.
static mut OIL_RESET_RANK_ADDR: u32 = 0;
// `GameTask::create_bubble_0` (0x005471B0) — usercall(EDI = descriptor,
// [stack] = parent, this), RET 0x8. EDI is callee-saved; the trampoline
// saves it across the call. The descriptor (7 dwords) is read by
// `SeaBubbleEntity::Constructor` (chained inside).
static mut OIL_CREATE_BUBBLE_ADDR: u32 = 0;

// Lifecycle bridges:
// `EntityActivityQueue::FreeSlotById` (0x00541860) — usercall(EAX=queue,
// [stack]=slot), RET 0x4.
static mut OIL_FREE_ACTIVITY_SLOT_ADDR: u32 = 0;
// `WorldEntity::Destructor` (0x004FEF30) — thiscall(this), plain RET.
// Larger / SEH-protected — kept bridged for now, port deferred.
static mut OIL_CGAMETASK_DESTRUCTOR_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        OIL_STEP_ROPE_PHYSICS_ADDR = rb(0x005003D0);
        OIL_ROPE_PHYSICS_TAIL_ADDR = rb(0x00500630);
        OIL_RESET_RANK_ADDR = rb(0x00541790);
        OIL_CREATE_BUBBLE_ADDR = rb(0x005471B0);
        OIL_FREE_ACTIVITY_SLOT_ADDR = rb(0x00541860);
        OIL_CGAMETASK_DESTRUCTOR_ADDR = rb(0x004FEF30);
    }
}

/// `__usercall(stdcall this on stack, AL = mode)`, RET 0x4. Bridge zeroes
/// AL explicitly before the call, matching WA's `XOR AL,AL` at the case-0x3
/// call site.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_step_rope_physics(_this: *mut OilDrumEntity) {
    core::arch::naked_asm!(
        "xor al, al",
        "push dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym OIL_STEP_ROPE_PHYSICS_ADDR,
    );
}

/// `__usercall(EAX = this)`, no stack args, plain RET. Tail companion to
/// `bridge_step_rope_physics` in case 0x03.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_rope_physics_tail(_this: *mut OilDrumEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym OIL_ROPE_PHYSICS_TAIL_ADDR,
    );
}

/// `EntityActivityQueue::ResetRank` (0x00541790) —
/// `__usercall(EAX = queue, [stack] = slot)`, RET 0x4.
#[unsafe(naked)]
pub(super) unsafe extern "stdcall" fn bridge_reset_rank(
    _queue: *mut EntityActivityQueue,
    _slot: i32,
) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "push dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 8",
        addr = sym OIL_RESET_RANK_ADDR,
    );
}

/// `GameTask::create_bubble_0` (0x005471B0) — `__usercall(EDI = descriptor,
/// [stack] = parent, this)`, RET 0x8. EDI is callee-saved; the trampoline
/// saves it across the call.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_create_bubble(
    _this: *mut OilDrumEntity,
    _parent: *mut u8,
    _descriptor: *const u32,
) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+16]", // descriptor
        "push dword ptr [esp+12]",     // parent
        "push dword ptr [esp+12]",     // this
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop edi",
        "ret 12",
        addr = sym OIL_CREATE_BUBBLE_ADDR,
    );
}

/// `EntityActivityQueue::FreeSlotById` (0x00541860) —
/// `__usercall(EAX = queue, [stack] = slot)`, RET 0x4.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_free_activity_slot(_queue: *mut EntityActivityQueue, _slot: i32) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "push dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 8",
        addr = sym OIL_FREE_ACTIVITY_SLOT_ADDR,
    );
}

/// `WorldEntity::Destructor` (0x004FEF30) — `__thiscall(this)`, plain RET.
#[inline]
unsafe fn bridge_cgametask_destructor(this: *mut OilDrumEntity) {
    type Fn = unsafe extern "thiscall" fn(*mut OilDrumEntity);
    let f: Fn = unsafe { core::mem::transmute(OIL_CGAMETASK_DESTRUCTOR_ADDR as usize) };
    unsafe { f(this) }
}

/// Pure-Rust port of `OilDrumEntity::Destructor_1` (inline in `Free`).
/// Restores own vtable, releases the activity-queue slot, then chains
/// into the parent `WorldEntity` destructor.
unsafe fn destructor_1(this: *mut OilDrumEntity) {
    unsafe {
        (*this).base.base.vtable = rb(super::OILDRUM_ENTITY_VTABLE) as *const OilDrumEntityVtable;

        let world = (*(this as *const BaseEntity)).world;
        let queue = core::ptr::addr_of_mut!((*world).entity_activity_queue);
        bridge_free_activity_slot(queue, (*this).activity_rank_slot);

        bridge_cgametask_destructor(this);
    }
}

/// Pure-Rust port of `OilDrumEntity::Free` (0x00504C80, vtable slot 1).
pub unsafe extern "thiscall" fn free(this: *mut OilDrumEntity, flags: u8) -> *mut OilDrumEntity {
    unsafe {
        destructor_1(this);
        if (flags & 1) != 0 {
            wa_free(this as *mut u8);
        }
        this
    }
}

/// Inline port of `GameTask::set_active` (0x00547ED0). Same shape as the
/// helper in `mine/handle_message.rs`; refreshes the two world-level
/// activity-watchdog timers when not already past `-mode`.
#[inline]
unsafe fn set_world_activity_timer(world: *mut GameWorld, mode: i32) {
    unsafe {
        if -mode <= (*world)._field_5dc {
            (*world)._field_5dc = mode;
        }
        if -mode <= (*world)._field_7e48 {
            (*world)._field_7e48 = mode;
        }
    }
}

/// Pure-Rust port of `Task_OilDrum::detonate` (0x00504DF0). Sends a fixed
/// `damage = 50, explosion_id = 100` blast to the world root, spawns 4
/// fire entities at the corners of an 8.0-px square around the drum, then
/// plays the OilDrumImpact sound (0x3B).
pub unsafe fn detonate(this: *mut OilDrumEntity) {
    unsafe {
        // FireEntity parent: SharedData::Lookup(0, 0x17, this). Distinct
        // from `WorldRootEntity::SHARED_DATA_KEY = (0, 0x14)` — the fire
        // pool registers itself under (0, 0x17). The blast still uses the
        // world root via [`create_explosion`]'s own internal lookup.
        let fire_parent = SharedDataTable::from_task(this as *const BaseEntity).lookup(0, 0x17);
        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;
        let source_team = (*this).source_team_index;

        // CreateExplosion(EAX=pos_x, ECX=pos_y, [stack] = explosion_id=100,
        // damage=50, caller_flag=0, source_team).
        create_explosion(
            pos_x,
            pos_y,
            this as *mut BaseEntity,
            100,
            50,
            0,
            source_team,
        );

        // Stack-built FireEntity init. Offsets +0x08/+0x0C are rewritten
        // per corner; everything else stays constant.
        let mut init = FireEntityInit {
            spawn_x: pos_x,
            spawn_y: pos_y,
            spawn_offset_x: Fixed::ZERO,
            spawn_offset_y: Fixed::ZERO,
            _flag_10: 0,
            kind: 4,
            _flag_18: 1,
            fp_collision_radius: Fixed(5000),
            fp_02: 100,
            fp_spread: 0xF,
            fp_04: 0,
            team_index: source_team,
        };

        const NEG_8: Fixed = Fixed(0xFFF80000_u32 as i32);
        const POS_8: Fixed = Fixed(0x00080000);
        // Corners in WA order: (-8,-8), (+8,-8), (-8,+8), (+8,+8).
        let corners = [
            (NEG_8, NEG_8),
            (POS_8, NEG_8),
            (NEG_8, POS_8),
            (POS_8, POS_8),
        ];
        for (off_x, off_y) in corners {
            init.spawn_offset_x = off_x;
            init.spawn_offset_y = off_y;
            let buf = wa_malloc(0xD8);
            if buf.is_null() {
                continue;
            }
            // WA only zeroes the first 0xB8 bytes; the trailing 0x20 bytes
            // are left untouched (the FireEntity ctor doesn't read them
            // before initialising).
            core::ptr::write_bytes(buf, 0, 0xB8);
            fire_entity_construct(buf as *mut FireEntity, fire_parent, &init, 0);
        }

        let _ = play_sound_local(
            this as *mut WorldEntity,
            SoundId(0x3B),
            5,
            Fixed::ONE,
            Fixed::ONE,
        );
    }
}

/// `EntityMessage::FrameFinish` (0x02). Parent dispatch followed by the
/// post-switch tick body of WA's `OilDrumEntity::HandleMessage`. The tail
/// either returns, frees, or detonates-then-frees.
unsafe fn msg_frame_finish_tick(
    this: *mut OilDrumEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        // Parent dispatch (sound-handle polling, child broadcast).
        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::FrameFinish,
            size,
            data,
        );

        let pos_x = (*this).base.pos_x.to_raw();
        let pos_y = (*this).base.pos_y.to_raw();

        let world = (*(this as *const BaseEntity)).world;

        // Wet → triggered=1 (drum is committed to detonating; subsequent
        // damage skips the threshold gate).
        if (*this).base._field_b0 != 0 {
            (*this).triggered = 1;
        }

        // Activity-queue rank refresh on motion + RecordLandingEvent +
        // set_active timer.
        if WorldEntity::is_moving_raw(this as *const WorldEntity) {
            let queue = core::ptr::addr_of_mut!((*world).entity_activity_queue);
            bridge_reset_rank(queue, (*this).activity_rank_slot);
            GameWorld::record_landing_event_raw(world, 0xC, pos_x, pos_y);
            set_world_activity_timer(world, 0xC);
        }

        // Underwater bubble emission. Each frame adds 0.25 to the
        // accumulator; on every full unit, a bubble is emitted and 1.0
        // is subtracted. Once the first bubble fires (and `triggered`
        // is still 0), the bucket_mask gets the one-time "underwater"
        // switch (`1 << 22`).
        if (*this).base._field_b0 != 0 {
            (*this).bubble_phase = Fixed((*this).bubble_phase.to_raw().wrapping_add(0x4000));
            while (*this).bubble_phase.to_raw() >= 0x10000 {
                (*this).bubble_phase = Fixed((*this).bubble_phase.to_raw().wrapping_sub(0x10000));
                let rng = (*world).advance_effect_rng();
                let kind = ((rng >> 16) & 3).wrapping_add(1);

                let parent = SharedDataTable::from_task(this as *const BaseEntity).lookup(0, 0x18);
                let descriptor: [u32; 7] = [0, pos_x as u32, pos_y as u32, 0, 0, 0, kind];
                bridge_create_bubble(this, parent, descriptor.as_ptr());
            }

            if (*this).triggered == 0 {
                (*this).base.bucket_mask = 0x400000;
                (*this).triggered = 1;
            }
        }

        // Off-bottom drop: when the drum has fallen past the kill plane
        // (`world.water_kill_y`, world+0x5E4), free without detonating.
        if (pos_y >> 16) >= (*world).water_kill_y {
            let mvt = *(this as *const *const OilDrumEntityVtable);
            ((*mvt).free)(this, 1);
            return;
        }

        // Otherwise: gate detonation on the SetTerminateFlag slot
        // (subclass_data[0xC] = entity offset 0x44). Zero → no detonate
        // requested → return without freeing. Non-zero → detonate, then
        // free via vtable slot 1.
        let detonate_flag = *((*this).base.subclass_data.as_ptr().add(0xC) as *const u32);
        if detonate_flag == 0 {
            return;
        }
        detonate(this);
        let mvt = *(this as *const *const OilDrumEntityVtable);
        ((*mvt).free)(this, 1);
    }
}

/// `EntityMessage::RenderScene` (0x03). Rope-physics step → render →
/// rope-physics tail → parent dispatch.
unsafe fn msg_render(
    this: *mut OilDrumEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        bridge_step_rope_physics(this);
        super::render::oil_drum_render(this);
        bridge_rope_physics_tail(this);
        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::RenderScene,
            size,
            data,
        );
    }
}

/// `EntityMessage::Explosion` (0x1C). When `triggered` is already set the
/// drum no longer accepts new damage — fall through to default. Otherwise
/// modern schemes (game_version > 0x4D) with `caller_flag != 0` need a
/// local copy of the message before forwarding so the parent doesn't
/// re-emit a kill-attribution report; legacy schemes (or `caller_flag == 0`)
/// mutate the original buffer in place.
unsafe fn msg_explosion(
    this: *mut OilDrumEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        if (*this).triggered != 0 {
            return;
        }

        let msg = data as *const ExplosionMessage;
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let game_version = (*game_info).game_version;

        if (*msg).caller_flag != 0 && game_version > 0x4D {
            // Modern path: local copy with caller_flag cleared.
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
            // Legacy path: mutate the original in place.
            (*(msg as *mut ExplosionMessage)).caller_flag = 0;
            world_entity_handle_message(
                this as *mut WorldEntity,
                sender,
                EntityMessage::Explosion,
                size,
                data,
            );
        }

        // Damage-accum readout: parent's accumulator landed in
        // [this+0xD4]. WA reads it, clears it, and if it was non-zero
        // requests detonation + captures the source team.
        let damage_accum = (*this).base.damage_accum;
        (*this).base.damage_accum = 0;
        if damage_accum == 0 {
            return;
        }
        request_detonate(this, (*msg).owner_id);

        // Old-scheme alliance gate: if both fire thresholds block damage
        // (>= 3), reset source_team to 0 so the eventual `detonate` blast
        // is anonymous. Modern schemes (`>= 0x1E6`) skip this fallback.
        if game_version < 0x1E6 {
            let ff = *((game_info as *const u8).add(0xD95C));
            let ef = *((game_info as *const u8).add(0xD95D));
            if ff >= 3 && ef >= 3 {
                (*this).source_team_index = 0;
            }
        }
    }
}

/// `EntityMessage::SpecialImpact` (0x4B).
///
/// - `flag != 1` → cosmetic / direct trigger; immediately request
///   detonation + capture source team.
/// - `flag == 1` → real impact; accumulate `damage` into
///   [`OilDrumEntity::damage_received`] and only detonate once the total
///   reaches [`OilDrumEntity::max_health`].
unsafe fn msg_special_impact(this: *mut OilDrumEntity, data: *const u8) {
    unsafe {
        if (*this).triggered != 0 {
            return;
        }
        let msg = data as *const SpecialImpactMessage;
        if (*msg).flag != 1 {
            // Direct trigger.
            request_detonate(this, (*msg).source_team_index);
            return;
        }
        // Threshold-gated path.
        (*this).damage_received = (*this).damage_received.wrapping_add((*msg).damage);
        if (*this).damage_received < (*this).max_health {
            return;
        }
        request_detonate(this, (*msg).source_team_index);
    }
}

/// Set the detonation-request flag (`subclass_data[0xC]`) and capture the
/// source team. Equivalent to WA's `vtable[14](this, 1); this+0x110 = team`.
#[inline]
unsafe fn request_detonate(this: *mut OilDrumEntity, source_team: u32) {
    unsafe {
        let dst = (*this).base.subclass_data.as_mut_ptr().add(0xC) as *mut u32;
        *dst = 1;
        (*this).source_team_index = source_team;
    }
}

pub unsafe extern "thiscall" fn handle_message(
    this: *mut OilDrumEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let Ok(msg) = EntityMessage::try_from(msg_type) else {
            return fall_through(this, sender, msg_type, size, data);
        };
        match msg {
            EntityMessage::FrameFinish => msg_frame_finish_tick(this, sender, size, data),
            EntityMessage::RenderScene => msg_render(this, sender, size, data),
            EntityMessage::Explosion => msg_explosion(this, sender, size, data),
            EntityMessage::SpecialImpact => msg_special_impact(this, data),
            other => {
                world_entity_handle_message(this as *mut WorldEntity, sender, other, size, data)
            }
        }
    }
}

unsafe fn fall_through(
    this: *mut OilDrumEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    let raw = ORIGINAL_HANDLE_MESSAGE.load(Ordering::Relaxed);
    debug_assert!(
        raw != 0,
        "OilDrumEntity::HandleMessage original ptr not initialized; vtable_replace! ran?"
    );
    let f: HandleMessageFn = unsafe { core::mem::transmute(raw as usize) };
    unsafe { f(this, sender, msg_type, size, data) }
}
