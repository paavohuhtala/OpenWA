//! Pure-Rust port of `OilDrumEntity::Constructor` (0x00504AF0,
//! `__usercall(ECX = y, [stack] = this, parent, x, level_gen_flag)`,
//! RET 0x10).
//!
//! Allocation + zero-init of the first 0x114 bytes is done by the caller
//! ([`SpawnObject`](https://example) at 0x00561E76 → `WA_MallocMemset(0x114)`).
//! The constructor itself is responsible for chaining into
//! `WorldEntity::Constructor`, acquiring an `EntityActivityQueue` slot,
//! installing the OilDrum vtable + class type byte, picking the bucket
//! mask from the scheme, performing the initial position commit, and
//! optionally running the level-gen one-pixel-at-a-time drop loop.
//!
//! Subsystem callees still bridged to WA:
//!  * `WorldEntity::Constructor` (0x004FED50) — large MFC-decorated init;
//!    not in scope for this slice.
//!  * `EntityActivityQueue::ResetRank` (0x00541790) — usercall(EAX=queue,
//!    [stack]=slot). Reused via `super::handle_message::bridge_reset_rank`.

use core::sync::atomic::{AtomicU32, Ordering};

use super::OilDrumEntity;
use super::handle_message::bridge_reset_rank;
use crate::engine::EntityActivityQueue;
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::game::class_type::ClassType;
use crate::rebase::rb;
use openwa_core::fixed::Fixed;

// `WorldEntity::Constructor` (0x004FED50) — `__stdcall(this, parent,
// class_type, flag)`, RET 0x10. Large MFC-style initializer; reused as a
// bridge until WorldEntity itself is ported.
static WORLD_ENTITY_CTOR_ADDR: AtomicU32 = AtomicU32::new(0);

pub unsafe fn init_addrs() {
    WORLD_ENTITY_CTOR_ADDR.store(rb(0x004FED50), Ordering::Relaxed);
}

#[inline]
unsafe fn world_entity_ctor(
    this: *mut WorldEntity<*const super::OilDrumEntityVtable, super::OilDrumSubclassData>,
    parent: *mut BaseEntity,
    class_type: u32,
    flag: u32,
) {
    type Fn = unsafe extern "stdcall" fn(
        *mut WorldEntity<*const super::OilDrumEntityVtable, super::OilDrumSubclassData>,
        *mut BaseEntity,
        u32,
        u32,
    );
    unsafe {
        let f: Fn = core::mem::transmute(WORLD_ENTITY_CTOR_ADDR.load(Ordering::Relaxed) as usize);
        f(this, parent, class_type, flag);
    }
}

/// Pure-Rust port of `OilDrumEntity::Constructor` (0x00504AF0). Caller
/// (`GameRuntime::CreateMines` / `SpawnObject`) has already allocated
/// `0x114` bytes and zeroed the whole block. Returns `this` (matching
/// WA's `MOV EAX, ESI`).
pub unsafe fn oil_drum_constructor(
    this: *mut OilDrumEntity,
    parent: *mut BaseEntity,
    x: i32,
    y: i32,
    level_gen_flag: u32,
) -> *mut OilDrumEntity {
    unsafe {
        // Parent ctor + class_type + vtable.
        world_entity_ctor(&raw mut (*this).base, parent, 0x11, 9);
        let world: *mut GameWorld = (*(this as *const BaseEntity)).world;
        (*(this as *mut BaseEntity)).class_type = ClassType::OilDrum;
        (*this).base.base.vtable =
            rb(super::OILDRUM_ENTITY_VTABLE) as *const super::OilDrumEntityVtable;

        // Caller already zero-initialised the whole 0x114; the constructor
        // explicitly sets these to mirror WA's writes (most are no-ops on a
        // zero-init buffer but the disasm spells them out, so we do too).
        (*this).triggered = 0;
        (*this).damage_received = 0;
        (*this).max_health = 0x32;
        (*this).bubble_phase = Fixed(0);

        // Acquire an activity-queue slot. WA inlines the acquire dance —
        // we use the typed helper which produces the identical effect.
        let queue: *mut EntityActivityQueue = &raw mut (*world).entity_activity_queue;
        let slot = EntityActivityQueue::acquire(queue);
        (*this).activity_rank_slot = slot;
        bridge_reset_rank(queue, slot);

        // Initial placement: probe `(x, y)`; commit on accept. WA writes
        // `bucket_mask = 2` first so the trial collision gets a known mask.
        (*this).base.bucket_mask = 2;
        WorldEntity::try_move_position_raw(this as *mut WorldEntity, x, y);

        // game_info byte at +0x7E40 selects two flag bits in the drum's
        // bucket_mask. SBB pattern transcribed verbatim.
        let scheme_byte: u8 = *((world as *const u8).add(0x7E40));
        let bit_20 = if scheme_byte >= 2 { 0x20u32 } else { 0 };
        let bit_10 = if scheme_byte >= 8 { 0x10u32 } else { 0 };
        (*this).base.bucket_mask = 0x0040180E | bit_20 | bit_10;

        // Subclass-data initial values. Caller already zero-filled the
        // whole 0x114; only the non-zero fields actually need writing.
        let sub = &raw mut (*this).base.subclass_data;
        (*sub)._field_3c = 1;
        (*sub).mass = Fixed::ONE;
        (*sub)._field_70 = 0x8000;

        // Pre-placed level-gen drums: drop one pixel at a time until
        // collision or the water surface, snapping `pos_x`/`pos_y` along
        // the way. Worm-placed drums (level_gen_flag == 0) skip this; the
        // initial `try_move_position` above is enough.
        if level_gen_flag != 0 {
            let mut y_step = y.wrapping_add(0x10000);
            while (y_step >> 16) < (*world).water_level {
                let collided =
                    !WorldEntity::check_move_collision_raw(this as *mut WorldEntity, x, y_step)
                        .is_null();
                if collided {
                    break;
                }
                if (*this).base._field_ac > 0 {
                    (*this).base._field_ac = 0;
                }
                (*this).base.pos_x = Fixed(x);
                (*this).base.pos_y = Fixed(y_step);
                y_step = y_step.wrapping_add(0x10000);
            }
        }

        this
    }
}
