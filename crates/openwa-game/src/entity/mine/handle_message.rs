//! Full port of `MineEntity::HandleMessage` (0x005072E0, vtable slot 2).
//!
//! Every explicit case plus the post-switch tick body (case `0x02
//! FrameFinish` fall-through). The original WA function is saved into
//! [`ORIGINAL_HANDLE_MESSAGE`] for unparseable messages only.

use core::sync::atomic::{AtomicU32, Ordering};

use super::{MineEntity, MineEntityVtable};
use crate::audio::KnownSoundId;
use crate::audio::{SoundId, sound_ops::play_sound_local};
use crate::engine::EntityActivityQueue;
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::game::create_explosion::create_explosion;
use crate::game::game_entity_message::{alliance_blocks_damage, world_entity_handle_message};
use crate::game::message::{EntityMessage, ExplosionMessage, SpecialImpactMessage};
use crate::rebase::rb;
use crate::render::textbox::Textbox;
use crate::wa_alloc::wa_free;
use openwa_core::fixed::Fixed;

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
// 0x00500630 — usercall(EAX = this), no stack args, plain RET. Tail
// companion to `StepRopePhysics_Maybe`. Was previously guessed as
// "RestoreKamikazeState" — that name was wrong.
static mut MINE_ROPE_PHYSICS_TAIL_ADDR: u32 = 0;

// Tick-body bridges (slice m2 + m3):
// `GameTask::ensure_recording` (0x00546B20) — usercall(EAX=this), plain RET.
static mut MINE_ENSURE_RECORDING_ADDR: u32 = 0;
// `GameTask::create_bubble_1` (0x005472C0) — usercall(EAX=pos_x, ECX=pos_y,
// ESI=this), 2 stack args (zero, kind), plain RET.
static mut MINE_CREATE_BUBBLE_ADDR: u32 = 0;
// `RandomBag::draw` (0x00541CC0) — thiscall(ECX=bag, [stack]=rng_value, out_ptr), RET 0x8.
static mut RANDOM_BAG_DRAW_ADDR: u32 = 0;
// `EntityActivityQueue::ResetRank` (0x00541790) — usercall(EAX=queue, [stack]=slot), RET 0x4.
static mut MINE_RESET_RANK_ADDR: u32 = 0;
// `GameTask::create_smoke_0` (0x00547490) — stdcall(this), RET 0x4. Reads
// EDI as a pointer to a 7-u32 spawn descriptor (preserved across the call
// and consumed by `SmokeEntity::Constructor`).
static mut MINE_CREATE_SMOKE_ADDR: u32 = 0;

// Lifecycle bridges (slice m6a):
// `EntityActivityQueue::FreeSlotById` (0x00541860) — usercall(EAX=queue, [stack]=slot), RET 0x4.
static mut MINE_FREE_ACTIVITY_SLOT_ADDR: u32 = 0;
// `WorldEntity::Destructor` (0x004FEF30) — thiscall(this), plain RET. Used
// by `MineEntity::Destructor_1` as the parent destructor chain. Larger /
// SEH-protected — kept bridged for now, port deferred.
static mut MINE_CGAMETASK_DESTRUCTOR_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        MINE_STEP_ROPE_PHYSICS_ADDR = rb(0x005003D0);
        MINE_ROPE_PHYSICS_TAIL_ADDR = rb(0x00500630);
        MINE_ENSURE_RECORDING_ADDR = rb(0x00546B20);
        MINE_CREATE_BUBBLE_ADDR = rb(0x005472C0);
        RANDOM_BAG_DRAW_ADDR = rb(0x00541CC0);
        MINE_RESET_RANK_ADDR = rb(0x00541790);
        MINE_CREATE_SMOKE_ADDR = rb(0x00547490);
        MINE_FREE_ACTIVITY_SLOT_ADDR = rb(0x00541860);
        MINE_CGAMETASK_DESTRUCTOR_ADDR = rb(0x004FEF30);
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

/// `GameTask::create_smoke_0` (0x00547490) — `__usercall(EDI = descriptor,
/// [stack] = this)`, RET 0x4. EDI is callee-saved, so the trampoline
/// saves it across the call. The descriptor is read by
/// `SmokeEntity::Constructor` (chained inside `create_smoke_0`).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_create_smoke(_this: *mut MineEntity, _descriptor: *const u32) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+12]", // descriptor
        "push dword ptr [esp+8]",      // this (callee cleans via RET 4)
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop edi",
        "ret 8",
        addr = sym MINE_CREATE_SMOKE_ADDR,
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
        addr = sym MINE_FREE_ACTIVITY_SLOT_ADDR,
    );
}

/// `WorldEntity::Destructor` (0x004FEF30) — `__thiscall(this)`, plain RET.
/// Parent-class destructor chain. Kept bridged: it does its own SEH +
/// children-list walk and is best ported in a dedicated WorldEntity slice.
#[inline]
unsafe fn bridge_cgametask_destructor(this: *mut MineEntity) {
    type Fn = unsafe extern "thiscall" fn(*mut MineEntity);
    let f: Fn = unsafe { core::mem::transmute(MINE_CGAMETASK_DESTRUCTOR_ADDR as usize) };
    unsafe { f(this) }
}

/// `GameTask::ensure_recording` (0x00546B20) —
/// `__usercall(EAX = this)`, plain RET. Returns 0 or 1 in EAX.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_ensure_recording(_this: *mut MineEntity) -> u32 {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym MINE_ENSURE_RECORDING_ADDR,
    );
}

/// `GameTask::create_bubble_1` (0x005472C0) —
/// `__usercall(EAX = pos_x, ECX = pos_y, ESI = this)`, 2 stack args
/// (`zero`, `kind`), plain RET. ESI is callee-saved.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_create_bubble(
    _this: *mut MineEntity,
    _pos_x: Fixed,
    _pos_y: Fixed,
    _kind: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",  // this
        "mov eax, dword ptr [esp+12]", // pos_x
        "mov ecx, dword ptr [esp+16]", // pos_y
        "push dword ptr [esp+20]",     // kind (re-push so callee sees it as stack arg)
        "push 0",                      // unknown leading zero arg
        "mov edx, dword ptr [{addr}]",
        "call edx",
        "pop esi",
        "ret 16",
        addr = sym MINE_CREATE_BUBBLE_ADDR,
    );
}

/// `RandomBag::draw` (0x00541CC0) — `__thiscall(ECX = bag,
/// [stack] = rng_value, out_ptr)`, RET 0x8. Picks an entry from the bag's
/// 100-element shuffle pool, writes it to `*out` and to the bag's drawn
/// history.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_random_bag_draw(_bag: *mut u8, _rng_value: u32, _out: *mut u32) {
    core::arch::naked_asm!(
        "mov ecx, dword ptr [esp+4]",  // bag
        "push dword ptr [esp+12]",     // out
        "push dword ptr [esp+12]",     // rng_value
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "ret 12",
        addr = sym RANDOM_BAG_DRAW_ADDR,
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
        addr = sym MINE_RESET_RANK_ADDR,
    );
}

/// Pure-Rust port of `MineEntity::Destructor_1` (0x00506AB0). Mirrors the
/// WA helper's deregistration order: re-establish the vtable slot,
/// release this mine's two world-level slots (`world._unknown_514[mine_list_slot]`
/// + the `EntityActivityQueue` rank), then in headful mode tear down the
/// per-mine textbox via [`Textbox::destroy`], and finally chain into the
/// parent `WorldEntity` destructor.
unsafe fn destructor_1(this: *mut MineEntity) {
    unsafe {
        // Re-establish own vtable so the parent destructor's virtual
        // dispatches resolve against MineEntity's slots, not whichever
        // descendant's slots were active before destruction started.
        (*this).base.base.vtable = rb(0x006643E8) as *const MineEntityVtable;

        // Clear the world-level mine-registry slot.
        let world = (*(this as *const BaseEntity)).world;
        *(*world).mine_list.add((*this).mine_list_slot as usize) = core::ptr::null_mut();

        // Release the EntityActivityQueue rank slot.
        let queue = core::ptr::addr_of_mut!((*world).entity_activity_queue);
        bridge_free_activity_slot(queue, (*this).activity_rank_slot as i32);

        if (*world).is_headful != 0 {
            Textbox::destroy((*this).textbox_handle);
        }

        // Parent destructor chain.
        bridge_cgametask_destructor(this);
    }
}

/// Pure-Rust port of `MineEntity::Free` (0x005069D0, vtable slot 1). Runs
/// the destructor and, when bit 0 of `flags` is set, frees the heap
/// allocation. Returns the `this` pointer in EAX (matches WA's calling
/// convention — the `extern "thiscall"` signature handles ECX = this and
/// the i8 stack arg).
pub unsafe extern "thiscall" fn free(this: *mut MineEntity, flags: u8) -> *mut MineEntity {
    unsafe {
        destructor_1(this);
        if (flags & 1) != 0 {
            wa_free(this as *mut u8);
        }
        this
    }
}

/// Pure-Rust port of `MineEntity::ScanForTrigger` (0x00507140). Walks the
/// world's triggerable-entity list at `world.game_state_stream + 0x20/+0x24`
/// and returns the first non-null entry that:
///
/// - has its `contact_face` low 5 bits set in the mine's
///   `trigger_class_mask` (the "trigger-class index" — populated by some
///   subclasses to opt into proximity triggers; **not** the BaseEntity
///   `class_type` enum at +0x20), and
/// - sits within `range` pixels of the mine in L1 distance
///   (`|dx_pixels| + |dy_pixels|`, with `dx`/`dy` shifted right 16 to drop
///   the fixed-point fraction before taking absolute value).
///
/// Returns `null` when the list is exhausted with no qualifying entry.
unsafe fn scan_for_trigger(this: *mut MineEntity, range: i32) -> *mut BaseEntity {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let stream = (*world).game_state_stream;
        let count = *(stream.add(0x20) as *const i32);
        let array = *(stream.add(0x24) as *const *mut *mut BaseEntity);

        let trigger_mask = (*this).trigger_class_mask;
        let this_x = (*this).base.pos_x.to_raw();
        let this_y = (*this).base.pos_y.to_raw();

        let mut idx: i32 = 0;
        loop {
            // Advance to the next non-null entry, exit when the list ends.
            let entry = loop {
                if idx >= count {
                    return core::ptr::null_mut();
                }
                let e = *array.offset(idx as isize);
                idx += 1;
                if !e.is_null() {
                    break e;
                }
            };

            let class_byte = (*(entry as *const crate::entity::WorldEntity)).contact_face;
            if (trigger_mask & (1u32 << (class_byte & 0x1F))) == 0 {
                continue;
            }

            let entry_x = *((entry as *const u8).add(0x84) as *const i32);
            let entry_y = *((entry as *const u8).add(0x88) as *const i32);
            let dx_pixels = (this_x.wrapping_sub(entry_x)) >> 16;
            let dy_pixels = (this_y.wrapping_sub(entry_y)) >> 16;
            let l1 = dx_pixels
                .wrapping_abs()
                .wrapping_add(dy_pixels.wrapping_abs());
            if l1 <= range {
                return entry;
            }
        }
    }
}

/// Pure-Rust port of `MineEntity::Arm` (0x00506CA0). Latches the settling
/// anim flag from `game_info._field_d780`, sets the unidentified armed
/// marker, and clears the arm-delay timer.
unsafe fn arm(this: *mut MineEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let sub = &raw mut (*this).base.subclass_data;
        (*sub).anim_flag = (*(*world).game_info)._field_d780;
        (*sub).armed_marker = 1;
        (*this).arm_delay = 0;
    }
}

/// Pure-Rust port of `MineEntity::Detonate` (0x00507110). Sends a fixed
/// `damage = 100` explosion through the world root with the mine as
/// sender. WA's call site adjusts `pos_y` by `+0x100000` (16.0 in fixed
/// point) so the blast originates above the mine sprite, not at its
/// pixel position.
unsafe fn detonate(this: *mut MineEntity) {
    unsafe {
        let pos_x = (*this).base.pos_x;
        let pos_y = Fixed((*this).base.pos_y.to_raw().wrapping_add(0x100000));
        create_explosion(
            pos_x,
            pos_y,
            this as *mut BaseEntity,
            100,
            (*this).damage as u32,
            0,
            (*this).placer_team_index as u32,
        );
    }
}

/// Pure-Rust port of `MineEntity::EmitDudSmoke` (0x00507210). Spawns 10
/// smoke particles in a small region around `(pos_x, pos_y)` via
/// `GameTask::create_smoke_0`. Each particle gets its own random sub-pixel
/// jitter and lifetime drawn from the secondary effect RNG; the spawn
/// descriptor is shared across iterations and re-filled in place.
unsafe fn emit_dud_smoke(this: *mut MineEntity, pos_x: Fixed, pos_y: Fixed) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let mut descriptor: [u32; 7] = [
            0x8FF00,
            pos_x.to_raw() as u32,
            pos_y.to_raw() as u32,
            0,
            0,
            0x267,
            0,
        ];
        for _ in 0..10 {
            let r1 = (*world).advance_effect_rng();
            let r2 = (*world).advance_effect_rng();
            let r3 = (*world).advance_effect_rng();
            descriptor[3] = (r1 & 0xFFFF).wrapping_sub(0x8000);
            descriptor[4] = (r2 & 0xFFFF).wrapping_sub(0x8000);
            // Magic-number divide by 200 (matches WA: `MUL 0x51EB851F; SHR EDX, 6`).
            descriptor[6] = ((r3 & 0xFFFF) / 200).wrapping_add(0x20C);
            bridge_create_smoke(this, descriptor.as_ptr());
        }
    }
}

/// Pure-Rust port of `MineEntity::RollFuseFromReplay` (0x00507B10, vtable
/// slot 19). When the fuse timer is still in its negative sentinel state,
/// rolls a fresh value in `[0, 3000)` ms via the gameplay RNG and records
/// it into the active replay/projectile-play log so playback reproduces
/// the same number.
unsafe fn roll_fuse_from_replay(this: *mut MineEntity) {
    unsafe {
        if (*this).fuse_timer >= 0 {
            return;
        }
        let world = (*(this as *const BaseEntity)).world;
        let rng = (*world).advance_rng();
        let new_fuse = (rng % 3000) as i32;
        (*this).fuse_timer = new_fuse;

        let idx = (*this)._field_194;
        if idx == u32::MAX {
            return;
        }
        // `world._unknown_51c` is the projectile-play log struct; +0x1C is
        // the data start pointer, +0x20 is the data end (capacity) pointer.
        let log = (*world)._unknown_51c;
        if log.is_null() {
            return;
        }
        let start = *(log.add(0x1C) as *const *mut u32);
        let end = *(log.add(0x20) as *const *mut u32);
        let count = if start.is_null() {
            0
        } else {
            end.offset_from(start) as usize
        };
        assert!(
            !start.is_null() && (idx as usize) < count,
            "MineEntity::RollFuseFromReplay: replay log index {idx} out of bounds (count={count})"
        );
        *start.add(idx as usize) = new_fuse as u32;
    }
}

/// Anim-flag write performed by both case 0x1C and case 0x4B when the mine
/// is still settling on a modern-enough scheme.
#[inline]
unsafe fn maybe_set_settling_anim_flag(this: *mut MineEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        if (*game_info).game_version > 0x3C && (*this).arm_delay > 0 {
            (*this).base.subclass_data.anim_flag = (*game_info)._field_d780;
        }
    }
}

/// `0x15 GameOver` — clear the trigger-armed flag and the triggered
/// latch, leaving the mine inert. Despite the enum name this fires at
/// round end, not match end.
unsafe fn msg_game_round_end(this: *mut MineEntity) {
    unsafe {
        (*this).trigger_armed_flag = 0;
        (*this).triggered_flag = 0;
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
        super::render::mine_render(this);
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

/// Speed-driven anim-phase update that produces the `anim_ticked` bool
/// the rest of the tick body branches on.
///
/// Modern path (`game_version >= 0x1D`): Mirrors the WA logic that
/// converts speed magnitude into a phase increment via `FixedDiv16_16`,
/// wraps the phase into `[0, 0x10000)`, and — when the input is so low
/// that the increment saturates at `0x199A` — gently snaps the phase
/// toward the frame-counter target so multiple stationary mines stay in
/// sync.
///
/// Old path (`game_version < 0x1D`): A simple "anim ticks every Nth
/// frame" rule, where N grows as the mine slows down.
unsafe fn step_anim_phase(this: *mut MineEntity, world: *mut GameWorld, game_version: i32) -> bool {
    unsafe {
        let speed_x = (*this).base.speed_x.to_raw();
        let speed_y = (*this).base.speed_y.to_raw();
        let abs_sx = speed_x.wrapping_abs();
        let abs_sy = speed_y.wrapping_abs();

        if game_version >= 0x1D {
            // Modern path. The compiler also computes a clamped
            // `inv_4x = (0x28000 - abs(sy) - abs(sx)) * 4` to use as the
            // FixedDiv16_16 denominator when above 0x10000, otherwise
            // saturates the increment to 0x10000.
            let inv_4x = 0x28000_i32
                .wrapping_sub(abs_sy)
                .wrapping_sub(abs_sx)
                .wrapping_mul(4);
            let step = if inv_4x < 0x10000 {
                0x10000_i32
            } else {
                // FixedDiv16_16(0x10000, inv_4x) = 0x100000000 / inv_4x.
                // Per the WA helper at 0x005B3501, the division is signed
                // 64-bit / 32-bit; inv_4x is positive here so an unsigned
                // path would be equivalent.
                ((0x100000000_i64 / inv_4x as i64) as i32).wrapping_add(1)
            };
            let phase = (*this)._field_190.wrapping_add(step as u32);
            (*this)._field_190 = phase;
            let anim_ticked = (phase as i32) >= 0x10000;
            if anim_ticked {
                (*this)._field_190 = phase.wrapping_sub(0x10000);
            }

            // Snap-to-target only fires when the increment saturated
            // (`step == 0x199A`) — i.e. abs(sx) + abs(sy) is ≤ ~16, the
            // range over which `0x100000000 / inv_4x + 1` rounds to
            // `0x199A`. The target tracks `(frame_counter % 10) * 0x199A`
            // so the phase realigns each 10-frame cycle.
            if step == 0x199A {
                let frame_counter = (*world).frame_counter;
                // CDQ + IDIV ECX(=10) → signed remainder with sign of dividend.
                let target = ((frame_counter % 10).wrapping_mul(0x199A) as u32 & 0xFFFF) as i32;
                let phase = (*this)._field_190 as i32;
                let mut diff = target.wrapping_sub(phase) & 0xFFFF;
                if diff != 0 {
                    if diff > 0x8000 {
                        diff = diff.wrapping_sub(0x10000);
                    }
                    let abs_diff = diff.wrapping_abs();
                    if abs_diff <= 0x51F {
                        (*this)._field_190 = target as u32;
                    } else {
                        (*this)._field_190 = (phase.wrapping_add(0x51F) as u32) & 0xFFFF;
                    }
                }
            }
            anim_ticked
        } else {
            // Old path: divisor = 10 - (abs_sx + abs_sy)*4 >> 16, clamped to ≥1.
            let speed_metric = abs_sx.wrapping_add(abs_sy).wrapping_mul(4) >> 16;
            let divisor = 10_i32.wrapping_sub(speed_metric).max(1);
            let frame_counter = (*world).frame_counter;
            (frame_counter % divisor) == 0
        }
    }
}

/// Inline port of `GameTask::set_active` (0x00547ED0).
///
/// Refreshes the two world-level activity-watchdog timers
/// ([`GameWorld::_field_5dc`] and [`GameWorld::_field_7e48`]) to `mode`,
/// but only when each timer has not already decayed past `-mode`. Used
/// by the mine tick whenever the mine is moving / armed / triggered to
/// keep the round's activity watchdogs alive.
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

/// `EntityMessage::FrameFinish` (0x02) — parent dispatch followed by
/// the entire post-switch tick body of WA's `MineEntity::HandleMessage`.
///
/// The structure follows the WA function 1:1:
/// 1. Block A — speed-driven anim phase update producing `anim_ticked`.
/// 2. Block B — three-way arm-delay state machine
///    (airborne / settling / armed).
///    - The "armed" leaf drives proximity scan, fuse countdown, beep
///      tier change, and end-of-fuse RNG-gated dud-vs-detonate decision.
/// 3. Block D — tail bookkeeping: off-bottom drop, activity-queue rank
///    refresh, optional underwater bubble emission, splash sound, final
///    "no longer moving" cleanup.
/// 4. End — either return, free, or detonate-then-free, depending on
///    where the previous blocks routed control.
unsafe fn msg_frame_finish_tick(
    this: *mut MineEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        // ---- Parent dispatch (sound-handle polling, child broadcast) ----
        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::FrameFinish,
            size,
            data,
        );

        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;
        let speed_x = (*this).base.speed_x;
        let speed_y = (*this).base.speed_y;

        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let game_version = (*game_info).game_version;

        // ---- Block A — speed-driven anim phase ----
        let anim_ticked = step_anim_phase(this, world, game_version);

        // ---- Block B — arm-delay state machine ----
        // Reaches the tail (block D) by default; the "fuse expired and
        // *not* a dud" path skips D and goes straight to detonate.
        let mut go_detonate_skip_tail = false;

        let arm_delay_v = (*this).arm_delay;

        if arm_delay_v < 0 {
            // B1 — airborne. Arm once the body comes to rest.
            if !WorldEntity::is_moving_raw(this as *const WorldEntity)
                && speed_x == Fixed::ZERO
                && speed_y == Fixed::ZERO
            {
                arm(this);
            }
            // → fall through to block D
        } else if arm_delay_v > 0 {
            // B2 — settling. Decrement 20/frame; arm when ≤ 0.
            let new_delay = arm_delay_v.wrapping_sub(0x14);
            (*this).arm_delay = new_delay;
            if new_delay <= 0 {
                arm(this);
            }
            // → fall through to block D
        } else if (*this).triggered_flag == 0 {
            // B3a — armed but not yet triggered.
            // Skip when underwater, when no anim tick this frame, or when
            // the trigger-armed flag was cleared by GameOver.
            if (*this).base._field_b0 == 0 && anim_ticked && (*this).trigger_armed_flag != 0 {
                let trigger_range = (*this).trigger_range as i32;
                let target = scan_for_trigger(this, trigger_range);
                if !target.is_null() {
                    let _ = bridge_ensure_recording(this);
                    let _ = play_sound_local(
                        this as *mut WorldEntity,
                        SoundId(0x58),
                        5,
                        Fixed::ONE,
                        Fixed::ONE,
                    );

                    // Capture placer team from the triggering entity when
                    // we don't already have one. The "should we capture"
                    // gate compares scheme bytes `+0xD95C` (friendly fire)
                    // and `+0xD95D` (enemy fire) and only captures when
                    // those say damage *would* propagate to allies.
                    if (*this).placer_team_index == 0 {
                        let ff = *((game_info as *const u8).add(0xD95C));
                        let ef = *((game_info as *const u8).add(0xD95D));
                        let do_capture = if game_version < 0x1E6 {
                            // Old gate: capture iff exactly one of the
                            // two bytes blocks damage (>= 3).
                            (ff >= 3) != (ef >= 3)
                        } else {
                            // New gate: capture iff at least one of the
                            // two scheme bytes blocks damage. Equivalent
                            // to the WA disasm's `JNC capture / JC skip /
                            // JMP capture` ladder.
                            ff >= 3 || ef >= 3
                        };
                        if do_capture {
                            // vtable[18] on the triggering entity returns
                            // its team index. The vtable for arbitrary
                            // BaseEntity subclasses is opaque here, so
                            // dispatch via raw pointer + offset.
                            type Vt18 = unsafe extern "thiscall" fn(*mut BaseEntity) -> i32;
                            let vt = *(target as *const *const u32);
                            let slot18 = *(vt.add(18));
                            let f: Vt18 = core::mem::transmute(slot18 as usize);
                            (*this).placer_team_index = f(target);
                        }
                    }

                    roll_fuse_from_replay(this);

                    (*this).triggered_flag = 1;

                    // Fall through into the beep-tier seed at LAB_005076B8.
                    (*this).beep_tier_index = (*this).fuse_timer / 250;
                }
            }
            // → fall through to block D
        } else {
            // B3b/c — armed and triggered.
            if (*this).base._field_b0 == 0 {
                // Above-water: count the fuse down at 20/frame.
                let new_fuse = (*this).fuse_timer.wrapping_sub(0x14);
                (*this).fuse_timer = new_fuse;

                if new_fuse > 0 {
                    // B3b — fuse still running. Beep on tier change.
                    let new_tier = new_fuse / 250;
                    if new_tier != (*this).beep_tier_index {
                        let _ = play_sound_local(
                            this as *mut WorldEntity,
                            SoundId(0x59),
                            5,
                            Fixed::ONE,
                            Fixed::ONE,
                        );
                        (*this).beep_tier_index = new_tier;
                    }
                } else {
                    // B3c — fuse expired. Roll the dud bag and decide.
                    let mut bag_value: u32 = 0;
                    let rng = (*world).advance_rng();
                    // `world+0x360C` is a [`RandomBag`]-shaped struct that
                    // sits inside [`GameWorld::_unknown_360c`]; not yet
                    // surfaced as a typed field.
                    let bag = (world as *mut u8).add(0x360C);
                    bridge_random_bag_draw(bag, rng, &mut bag_value);

                    // Dud branch all-of guards (any miss → real detonate):
                    //   _field_108 == 0      (something else already steered toward boom)
                    //   ScanForTrigger(damage*2 + 10) returns a hit  (a worm is right next to us)
                    //   game_info.duds_enabled != 0   (scheme has duds enabled)
                    //   bag_value != 0       (bag-drawn value picked the dud slot)
                    //   is_not_dud == 0      (mine wasn't worm-placed)
                    let radius = (*this).damage.wrapping_mul(2).wrapping_add(10);
                    let nearby = scan_for_trigger(this, radius);

                    let is_dud = (*this)._field_108 == 0
                        && !nearby.is_null()
                        && (*game_info).duds_enabled != 0
                        && bag_value != 0
                        && (*this).is_not_dud == 0;

                    if is_dud {
                        // Dud — clear fuse/triggered, mark as fled, play
                        // the dud sound + smoke, fall to block D.
                        (*this).fuse_timer = 0;
                        (*this).triggered_flag = 0;

                        // `trigger_range = (game_version < 0x1F) - 1` —
                        // repurposes the slot as a "post-dud marker" for
                        // downstream code (so it doesn't round-trip to a
                        // real range). Old schemes get 0; modern schemes
                        // get -1. Earlier Rust port had the branches
                        // inverted; caught by the dual-pass `[MineDump]`
                        // diff on `bomber_parachute` frame 470.
                        (*this).trigger_range =
                            ((game_version < 0x1F) as i32).wrapping_sub(1) as u32;
                        (*this).fled = 1;

                        // PlaySoundLocal(0x5A, 5, vol=1.0, pitch=2.0)
                        let _ = play_sound_local(
                            this as *mut WorldEntity,
                            SoundId(0x5A),
                            5,
                            Fixed::ONE,
                            Fixed(0x20000),
                        );
                        emit_dud_smoke(this, pos_x, pos_y);
                    } else {
                        // Real detonate — skip block D entirely.
                        go_detonate_skip_tail = true;
                    }
                }
            }
            // → fall through to block D (unless we set go_detonate_skip_tail)
        }

        if go_detonate_skip_tail {
            detonate(this);
            // Free.
            let mvt = *(this as *const *const MineEntityVtable);
            ((*mvt).free)(this, 1);
            return;
        }

        // ---- Block D — tail bookkeeping ----
        // Off-bottom drop: when the mine has fallen past the kill plane,
        // free without detonating.
        if pos_y.to_int() >= (*world).water_kill_y {
            MineEntity::free_raw(this, 1);
            return;
        }

        // Activity-queue rank refresh: any motion / triggered / settling
        // state forces a "newest" promotion plus the world's activity
        // timer reset.
        let any_active = WorldEntity::is_moving_raw(this as *const WorldEntity)
            || (*this).triggered_flag != 0
            || (*this).arm_delay != 0;
        if any_active {
            let queue = core::ptr::addr_of_mut!((*world).entity_activity_queue);
            bridge_reset_rank(queue, (*this).activity_rank_slot as i32);

            // RecordLandingEvent: idx = 10 if underwater, else 5.
            let idx = if (*this).base._field_b0 != 0 { 10 } else { 5 };
            GameWorld::record_landing_event_raw(world, idx, pos_x, pos_y);

            // GameTask::set_active(this, mode=0xC).
            set_world_activity_timer(world, 0xC);
        }

        // Underwater bubble emission. Each frame adds 0.25 to the
        // accumulator; on every full unit, a bubble is emitted and 1.0
        // is subtracted. The first transition into water also rewrites
        // `bucket_mask = 1 << 22` as a one-time init — plausibly
        // switching the mine to a water-specific collision bucket so it
        // continues to sink and interact with water-side collidables.
        if (*this).base._field_b0 != 0 {
            (*this).bubble_phase = Fixed((*this).bubble_phase.to_raw().wrapping_add(0x4000));
            while (*this).bubble_phase.to_raw() >= 0x10000 {
                (*this).bubble_phase = Fixed((*this).bubble_phase.to_raw().wrapping_sub(0x10000));
                let rng = (*world).advance_effect_rng();
                let kind = ((rng >> 16) & 3).wrapping_add(1);
                bridge_create_bubble(this, pos_x, pos_y, kind);
            }
            if (*this)._field_10c == 0 {
                (*this).base.bucket_mask = 0x400000;
                (*this)._field_10c = 1;
            }
        }

        // Splash sound + wet-flag bookkeeping.
        if (*this).splash_played == 0 && (*this).base._field_a4 != 0 {
            (*this).splash_played = 1;
            if speed_y > Fixed::ONE {
                let _ = play_sound_local(
                    this as *mut WorldEntity,
                    KnownSoundId::Splash,
                    5,
                    Fixed::ONE,
                    Fixed::ONE,
                );
            }
        }
        if (*this).splash_played != 0 && (*this).base._field_a4 == 0 {
            (*this).splash_played = 0;
        }

        // "No longer moving" cleanup.
        if !WorldEntity::is_moving_raw(this as *const WorldEntity) {
            (*this)._field_108 = 0;
        }

        // Final outcome: `terminate_flag` (mine offset 0x44) gates
        // detonation. When zero, the tick simply returns; when non-zero,
        // the mine detonates (and then frees).
        if (*this).base.subclass_data.terminate_flag == 0 {
            return;
        }
        detonate(this);
        let mvt = *(this as *const *const MineEntityVtable);
        ((*mvt).free)(this, 1);
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
            EntityMessage::FrameFinish => msg_frame_finish_tick(this, sender, size, data),
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
