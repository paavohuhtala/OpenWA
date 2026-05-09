use super::base::BaseEntity;
use super::game_entity::WorldEntity;
use crate::FieldRegistry;
use openwa_core::fixed::Fixed;

pub mod constructor;
pub mod handle_message;
pub mod render;

crate::define_addresses! {
    class "OilDrumEntity" {
        /// OilDrumEntity vtable - oil drum entity
        vtable OILDRUM_ENTITY_VTABLE = 0x00664338;
        ctor OILDRUM_ENTITY_CTOR = 0x00504AF0;
    }
}

/// OilDrumEntity vtable — 20 slots (extends WorldEntity's 20-slot layout
/// with oil-drum-specific overrides at slots 0/1/2/7/8/18). Only the slots
/// dispatched from Rust are spelled out.
///
/// Vtable at Ghidra 0x00664338.
#[openwa_game::vtable(size = 20, va = 0x00664338, class = "OilDrumEntity")]
pub struct OilDrumEntityVtable {
    /// `OilDrumEntity::Free` (0x00504C80) — scalar deleting destructor.
    /// Called by `HandleMessage` after detonation, after off-bottom drop,
    /// or after the cosmetic-impact path's tail.
    #[slot(1)]
    pub free: fn(this: *mut OilDrumEntity, flags: u8) -> *mut OilDrumEntity,
    /// HandleMessage — processes oil drum messages.
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(
        this: *mut OilDrumEntity,
        sender: *mut BaseEntity,
        msg_type: u32,
        size: u32,
        data: *const u8,
    ),
    /// `WorldEntity::SetTerminateFlag_Maybe` (0x004FE060) — generic
    /// terminator slot inherited from WorldEntity. Writes `flag` to
    /// `[this+0x44]`; case 0x1C / 0x4B in HandleMessage call this with
    /// `flag = 1` to request detonation in the next FrameFinish tick.
    /// thiscall(this, flag), RET 0x4.
    #[slot(14)]
    pub set_terminate_flag: fn(this: *mut OilDrumEntity, flag: u32),
}

/// Exploding oil drum entity.
///
/// Extends WorldEntity (0xFC bytes). Oil drums sit on terrain, tip over
/// when shoved, take damage from explosions / special impacts, and detonate
/// once accumulated damage reaches the configured health threshold. Total
/// size 0x114 bytes (the alloc site at `SpawnObject` 0x00561E76 sets
/// `EDI = 0x114` before calling [`WA_MallocMemset`]).
///
/// Constructor: `OilDrumEntity::Constructor` (0x00504AF0,
/// `__usercall(ECX = y, [stack] = this, parent, x, level_gen_flag)`,
/// RET 0x10).
/// Vtable: 0x00664338. Class type byte: 0x1E.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct OilDrumEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94).
    /// Detonation-request flag lives in `subclass_data[0xC]`
    /// (entity offset 0x44) — written by vtable slot 14
    /// (`WorldEntity::SetTerminateFlag_Maybe`); read by `HandleMessage`'s
    /// FrameFinish tail to gate the explode-then-free path.
    pub base: WorldEntity<*const OilDrumEntityVtable>,
    /// 0xFC: "Started rolling / venting" latch. Set to 1 the first frame
    /// the drum is wet (`WorldEntity._field_b0 != 0`); also set by the
    /// underwater-bubble emitter the first frame it fires (alongside the
    /// `bucket_mask = 1 << 22` water-bucket switch). Once non-zero, all
    /// damage messages (Explosion / SpecialImpact) skip the threshold
    /// gate — the drum is committed to detonating.
    pub triggered: u32,
    /// 0x100: This drum's slot ID in `GameWorld.entity_activity_queue`,
    /// or `-1` when the queue's free pool was empty at construction time
    /// (rare; treated as "uncached, fall back to capacity/count" by
    /// [`render::oil_drum_render`]).
    pub activity_rank_slot: i32,
    /// 0x104: Damage accumulator. Incremented per `EntityMessage::SpecialImpact`
    /// (msg 0x4B) by the message's `damage` field; the drum detonates when
    /// the total reaches [`max_health`]. Also drives the body-sprite frame
    /// (4-step ladder, `0x6E .. 0x71`).
    ///
    /// [`max_health`]: Self::max_health
    pub damage_received: i32,
    /// 0x108: Health threshold. Initialised to `0x32` (50) by the
    /// constructor; the drum survives until [`damage_received`] >= this.
    ///
    /// [`damage_received`]: Self::damage_received
    pub max_health: i32,
    /// 0x10C: Underwater bubble-emission accumulator (Fixed 16.16). The
    /// FrameFinish tick adds `0.25` (`0x4000`) per frame; whenever it
    /// reaches 1.0, `GameTask::create_bubble_0` is called and the
    /// accumulator decrements by 1.0. Active only while underwater
    /// (`WorldEntity._field_b0 != 0`).
    pub bubble_phase: Fixed,
    /// 0x110: Source team index of the worm/team that triggered the drum.
    /// Initialised to `0` (anonymous level-gen origin) by the constructor;
    /// captured from the inbound `ExplosionMessage::owner_id` (case 0x1C)
    /// or `SpecialImpactMessage::source_team_index` (case 0x4B) the first
    /// frame the drum takes real damage. Used as the `source_team` field
    /// of the `Explosion` broadcast emitted by [`detonate`]. Cleared back
    /// to `0` for old schemes (`game_version < 0x1E6`) when both the
    /// friendly- and enemy-fire thresholds block damage — preserves the
    /// "anonymous detonation" fallback.
    ///
    /// [`detonate`]: handle_message::detonate
    pub source_team_index: u32,
}

const _: () = assert!(core::mem::size_of::<OilDrumEntity>() == 0x114);

// Generate typed vtable method wrappers: free(), handle_message(),
// set_terminate_flag().
bind_OilDrumEntityVtable!(OilDrumEntity, base.base.vtable);

impl OilDrumEntity {
    /// Returns true if the drum is on fire (subclass overload of
    /// `WorldEntity::_field_b0`).
    ///
    /// # Safety
    /// `self` must be a valid, fully-constructed OilDrumEntity.
    pub unsafe fn on_fire(&self) -> bool {
        self.base._field_b0 != 0
    }
}
