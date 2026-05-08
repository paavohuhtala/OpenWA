use super::base::BaseEntity;
use super::game_entity::WorldEntity;
use crate::FieldRegistry;

crate::define_addresses! {
    class "MineEntity" {
        ctor MINE_ENTITY_CTOR = 0x00506660;
    }
}

/// MineEntity vtable — 12 slots. Extends WorldEntity vtable with mine behavior.
///
/// Vtable at Ghidra 0x6643E8.
#[openwa_game::vtable(size = 12, va = 0x006643E8, class = "MineEntity")]
pub struct MineEntityVtable {
    /// HandleMessage — processes mine messages (arm, trigger, detonate).
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(
        this: *mut MineEntity,
        sender: *mut BaseEntity,
        msg_type: u32,
        size: u32,
        data: *const u8,
    ),
    /// ProcessFrame — per-frame mine update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut MineEntity, flags: u32),
}

/// Land mine entity.
///
/// Extends WorldEntity (0xFC bytes). Mines sit on the terrain and arm after
/// placement; they detonate on contact once armed.
///
/// Constructor: 0x506660 (stdcall). Allocates 0x1BC bytes; zero-inits only
/// the first 0x19C — the trailing 0x20 bytes are scratch the runtime never
/// reads.
/// Vtable: 0x6643E8. Class type byte: 0x08.
///
/// Source: Ghidra decompilation of 0x506660 (constructor) and
///         0x5072E0 (HandleMessage).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MineEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: WorldEntity<*const MineEntityVtable>,
    /// 0xFC–0x103
    pub _unknown_fc: [u8; 0x8],
    /// 0x104: Trigger-armed flag — set to 1 in ctor; cleared on
    /// `EntityMessage::GameOver` (msg 0x15). Tick body gates the
    /// proximity-trigger check on this; once cleared, the mine becomes
    /// inert (even if a worm walks over it).
    pub _field_104: u32,
    /// 0x108–0x10F
    pub _unknown_108: [u8; 0x8],
    /// 0x110: This mine's slot ID in `GameWorld.entity_activity_queue`.
    pub activity_rank_slot: u32,
    /// 0x114
    pub _unknown_114: u32,
    /// 0x118: Fuse timer (signed). Decrements 20/frame after the mine
    /// arms; detonates at ≤ 0.
    pub fuse_timer: i32,
    /// 0x11C: Settle / arm-delay timer (signed; seeded from
    /// `WeaponFireParams[1]`). Negative = airborne (arms when speed is
    /// zero). Positive = ground-settle countdown decrementing 20/frame
    /// (arms at ≤ 0). Zero = armed.
    pub _unknown_11c: u32,
    /// 0x120
    pub _unknown_120: u32,
    /// 0x124: Owner team index (`WeaponFireParams[6]`; -1 = no owner).
    pub owner_team: i32,
    /// 0x128: Triggered flag — cleared on `EntityMessage::GameOver`
    /// (msg 0x15); set in the tick body once a worm walks within trigger
    /// range and the fuse starts running.
    pub _field_128: u32,
    /// 0x12C–0x143
    pub _unknown_12c: [u8; 0x18],
    /// 0x144: Placer's team index — initialized in the constructor from
    /// `WeaponReleaseContext.team_id` (the team of the worm that placed
    /// the mine). Pre-placed level-gen mines are anonymous (team 0); the
    /// tick body has a fallback that captures the team of the triggering
    /// worm via its vtable[18] only when this slot is still zero.
    /// Used by `EntityMessage::Explosion` (0x1C) and
    /// `EntityMessage::SpecialImpact` (0x4B) as the *receiver* side of
    /// the alliance gate, against the message's sender team: same
    /// alliance reads `game_info+0xD95C` (friendly fire), cross-alliance
    /// reads `game_info+0xD95D` (enemy fire); a value > 2 cuts off the
    /// damage broadcast — so a mine you (or an ally) detonated won't be
    /// damaged by your own blast under friendly-fire-off schemes.
    pub placer_team_index: i32,
    /// 0x148–0x18F: Init-data tail. Ctor block-copies
    /// `WeaponReleaseContext[1..=10]` to 0x148–0x16F and
    /// `WeaponFireParams[0..=7]` to 0x170–0x18F. Surface as the tick body
    /// references these in slice m2.
    pub _unknown_148: [u8; 0x48],
    /// 0x190: Animation phase counter; seeded from `(rng % 10) * 0x199A`
    /// and advanced each tick.
    pub _field_190: u32,
    /// 0x194: ProjectilePlay tracking index — sentinel `0xFFFFFFFF` until
    /// the mine registers itself with the active replay/projectile-play log.
    pub _field_194: u32,
    /// 0x198–0x1BB: Heap allocator only zeroes the first 0x19C bytes;
    /// nothing in the constructor or HandleMessage reads or writes this
    /// range.
    pub _unknown_198: [u8; 0x24],
}

const _: () = assert!(core::mem::size_of::<MineEntity>() == 0x1BC);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_MineEntityVtable!(MineEntity, base.base.vtable);
