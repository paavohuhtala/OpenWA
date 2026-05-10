use super::base::BaseEntity;
use super::game_entity::{SubclassData, WorldEntity};
use crate::FieldRegistry;
use crate::render::textbox::Textbox;
use openwa_core::fixed::Fixed;

pub mod constructor;
pub mod handle_message;
pub mod render;

crate::define_addresses! {
    class "MineEntity" {
        ctor MINE_ENTITY_CTOR = 0x00506660;
    }
}

/// MineEntity's typed view of [`WorldEntity::subclass_data`]
/// (entity offsets +0x38..+0x84, 0x4C bytes total).
///
/// Touched by [`constructor::mine_constructor`] (initial values),
/// [`handle_message::arm`] (anim flag + armed marker), and the FrameFinish
/// tail in [`handle_message`] (reads `terminate_flag` to gate detonation).
/// The terminator slot itself is written through the inherited vtable slot
/// 14 (`WorldEntity::SetTerminateFlag_Maybe`).
#[repr(C)]
pub struct MineSubclassData {
    /// Entity +0x38: Unknown.
    pub _unknown_38: u32,
    /// Entity +0x3C: Initialised to `1` by the constructor; readers
    /// unidentified. Mirrors the same offset in `OilDrumEntity`'s subclass
    /// block (also written `1` by the drum ctor).
    pub _field_3c: u32,
    /// Entity +0x40: "Armed" marker. Set to `1` by [`handle_message::arm`]
    /// (`MineEntity::Arm` 0x00506CA0); the constructor also sets it to `1`
    /// directly when the mine spawns already settled (`arm_delay <= 0`).
    /// Distinct from the end-of-tick detonation gate
    /// ([`Self::terminate_flag`]); canonical purpose pending follow-up RE.
    pub armed_marker: u32,
    /// Entity +0x44: Detonation-request flag. Read by the FrameFinish tail
    /// to gate `detonate` + `free` — when zero the tick simply returns.
    /// Writers go through the inherited vtable slot 14
    /// (`WorldEntity::SetTerminateFlag_Maybe`); no Rust port writes it
    /// directly. Mirrors the same offset in `OilDrumEntity`'s subclass
    /// block.
    pub terminate_flag: u32,
    /// Entity +0x48: Unknown.
    pub _unknown_48: u32,
    /// Entity +0x4C: Mass (Fixed 16.16). Initialised to `1.0` by the
    /// constructor; consumed by `WorldEntityVtable::add_impulse` (slot 17)
    /// which divides each axis of the impulse by mass before accumulating
    /// into `speed_x`/`speed_y`.
    pub mass: Fixed,
    /// Entity +0x50: Unknown.
    pub _unknown_50: u32,
    /// Entity +0x54: Position-derived seed. Initialised by the constructor
    /// to `((spawn_x + spawn_y) >> 8 & 0xFFFF) / 20 + 0xCCCC`. Readers
    /// unidentified.
    pub position_seed: u32,
    /// Entity +0x58..+0x6B: Unknown (5 dwords).
    pub _unknown_58: [u32; 5],
    /// Entity +0x6C: Initialised to `0x9999` by the constructor; readers
    /// unidentified. Mirrors the same offset in `OilDrumEntity`'s subclass
    /// block (which the drum ctor leaves zero).
    pub _field_6c: u32,
    /// Entity +0x70: Initialised to `0x9999` by the constructor; readers
    /// unidentified. Mirrors the same offset in `OilDrumEntity`'s subclass
    /// block (which the drum ctor sets to `0x8000`).
    pub _field_70: u32,
    /// Entity +0x74: Animation flag. `WorldEntity::Constructor` seeds this
    /// from `GameInfo._field_d780`; the mine constructor immediately
    /// clears it back to `0` and only re-applies the `GameInfo` value when
    /// the mine spawns already settled (`arm_delay <= 0`). Also rewritten
    /// by [`handle_message::arm`] and by case 0x1C / 0x4B of
    /// [`handle_message`] when the mine is still settling on a modern
    /// scheme (`game_version > 0x3C`). Meaning otherwise opaque.
    pub anim_flag: u32,
    /// Entity +0x78..+0x83: Unknown (3 dwords).
    pub _unknown_78: [u32; 3],
}

const _: () = assert!(core::mem::size_of::<MineSubclassData>() == 0x4C);

unsafe impl SubclassData for MineSubclassData {}

/// MineEntity vtable — 32 slots. Extends WorldEntity's 20-slot vtable with
/// 12 mine-specific overrides.
///
/// Vtable at Ghidra 0x6643E8.
#[openwa_game::vtable(size = 32, va = 0x006643E8, class = "MineEntity")]
pub struct MineEntityVtable {
    /// `BaseEntity::Free` (0x005069D0 for MineEntity) — scalar deleting
    /// destructor. Called by `HandleMessage` after a real detonation, after
    /// an off-bottom drop, or from the dud-smoke-and-flee path's tail.
    #[slot(1)]
    pub free: fn(this: *mut MineEntity, flags: u8) -> *mut MineEntity,
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
    /// `MineEntity::RollFuseFromReplay` (0x00507B10) — rolls a fresh fuse
    /// timer from the gameplay RNG when `fuse_timer < 0`. Called from the
    /// tick body the moment a worm walks into trigger range, just before
    /// the mine sets `_field_128 = 1` (triggered). For mines that already
    /// have a non-negative fuse from `WeaponFireParams`, this is a no-op.
    /// The new fuse is also recorded into the active replay log via
    /// `_field_194` so playback reproduces the same number.
    ///
    /// Ported pure-Rust in slice m3 — the tick now calls the Rust impl
    /// directly; this slot is retained for type/registry metadata.
    #[slot(19)]
    pub roll_fuse_from_replay: fn(this: *mut MineEntity),
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
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94).
    /// Subclass-data overlay typed as [`MineSubclassData`] — exposes
    /// `mass` (entity +0x4C), the `armed_marker`/`terminate_flag` gates,
    /// the `anim_flag` (entity +0x74) etc. as named fields.
    pub base: WorldEntity<*const MineEntityVtable, MineSubclassData>,
    /// 0xFC: Frame the mine was inserted into the world's mine registry,
    /// snapshotted from `GameWorld::frame_counter` (+0x5CC) by
    /// `MineEntity::InsertIntoMineList`. Used as the LRU age key when the
    /// registry is full and a placement has to evict an older mine.
    pub inserted_frame: i32,
    /// 0x100: This mine's slot index in
    /// [`GameWorld::mine_list`](crate::engine::world::GameWorld#mine_list).
    /// Set by `MineEntity::InsertIntoMineList`; the destructor zeros
    /// `world.mine_list[mine_list_slot]` to deregister.
    pub mine_list_slot: u32,
    /// 0x104: Trigger-armed flag — set to 1 in ctor; cleared on
    /// `EntityMessage::GameOver` (msg 0x15). Tick body gates the
    /// proximity-trigger check on this; once cleared, the mine becomes
    /// inert (even if a worm walks over it).
    pub trigger_armed_flag: u32,
    /// 0x108: Persistence flag set by some external path; cleared by the
    /// tick body's tail whenever `WorldEntity::IsMoving` reports false.
    /// While non-zero it forces the dud-roll branch in B3c to skip the
    /// scan/duration fairness checks and detonate immediately, regardless
    /// of the bag draw or scheme `duds_enabled` byte. Writers and exact
    /// purpose are not yet identified.
    pub _field_108: u32,
    /// 0x10C: Underwater init-once flag. The tick's bubble-emission tail
    /// sets this to 1 the first frame the mine enters water and writes
    /// `bucket_mask = 1 << 22`, plausibly switching the mine's collision
    /// target set from the dry-terrain buckets it was constructed with
    /// to a water-specific bucket so it continues to sink and interact
    /// with water-side collidables. Subsequent underwater frames see the
    /// flag set and skip the one-time write. Confirming "bucket 22 ==
    /// water" is pending follow-up RE.
    pub _field_10c: u32,
    /// 0x110: This mine's slot ID in `GameWorld.entity_activity_queue`.
    pub activity_rank_slot: u32,
    /// 0x114: Trigger class bitmask. `MineEntity::ScanForTrigger` reads
    /// the candidate entity's `WorldEntity::contact_face` (entity offset
    /// `+0x30`), takes its low 5 bits as a "trigger-class index", and
    /// gates the proximity hit on
    /// `(trigger_class_mask >> trigger_class_index) & 1`. The index is
    /// **not** the BaseEntity `class_type` enum (which lives at +0x20);
    /// it's a separate per-subclass byte that triggerable entities write
    /// into their `contact_face`. Sourced from `WeaponFireParams[2]`
    /// for `FireType::Placed`.
    pub trigger_class_mask: u32,
    /// 0x118: Fuse timer (signed). Decrements 20/frame after the mine
    /// triggers; detonates at ≤ 0.
    pub fuse_timer: i32,
    /// 0x11C: Settle / arm-delay timer (signed; seeded from
    /// `WeaponFireParams[1]`). Negative = airborne (arms when speed is
    /// zero). Positive = ground-settle countdown decrementing 20/frame
    /// (arms at ≤ 0). Zero = armed-and-scanning.
    pub arm_delay: i32,
    /// 0x120: Trigger range in pixels (L1 distance — `|dx| + |dy|`).
    /// Passed to `MineEntity::ScanForTrigger` once the mine is armed;
    /// any qualifying entity within this radius triggers detonation.
    /// Sourced from `WeaponFireParams[0]` for `FireType::Placed`.
    pub trigger_range: u32,
    /// 0x124: Explosion damage at center, sourced from
    /// `WeaponFireParams[6]` for `FireType::Placed`. Passed straight
    /// through as `ExplosionMessage::damage` by `MineEntity::Detonate`
    /// (0x00507110). The dud-roll secondary scan in the tick uses
    /// `damage * 2 + 10` pixels as a "is anyone close enough to
    /// actually take damage" gate.
    pub damage: i32,
    /// 0x128: Triggered flag — cleared on `EntityMessage::GameOver`
    /// (msg 0x15); set in the tick body once a worm walks within trigger
    /// range and the fuse starts running.
    pub triggered_flag: u32,
    /// 0x12C: Beep-tier index — `fuse_timer / 250`. The tick body plays
    /// sound `0x59` (beep) once per tier change so the warning beep
    /// accelerates as the fuse counts down.
    pub beep_tier_index: i32,
    /// 0x130: Splash-played latch. Set to 1 the first frame the mine is
    /// "wet" (`WorldEntity._field_a4 != 0`) and `speed_y > 1.0`; the same
    /// frame plays sound `0x39`. Cleared back to 0 when the mine leaves
    /// water (`_field_a4 == 0`) so the next splash will play.
    pub splash_played: u32,
    /// 0x134: Currently unread by the constructor or HandleMessage —
    /// candidate for further RE.
    pub _field_134: u32,
    /// 0x138: "Fled" latch. Set to 1 by the dud-smoke-and-flee branch in
    /// B3c. Read by other systems but not by the tick body itself; the
    /// canonical reader has not been identified.
    pub fled: u32,
    /// 0x13C: "Worm-placed" flag — set to 1 by `fire_mine` (the Mine /
    /// MineStrike weapon paths). Pre-placed level-generation mines pass
    /// 0 here. The tick's dud branch gates on this being 0, so worm-
    /// placed mines never roll for dud at fuse end.
    pub is_not_dud: u32,
    /// 0x140: Underwater bubble-emission accumulator (Fixed 16.16). The
    /// tick adds `0.25` (`0x4000`) per frame; whenever it reaches 1.0,
    /// `GameTask::create_bubble_1` is called and the accumulator
    /// decrements by 1.0. Active only while `WorldEntity._field_b0 != 0`
    /// (mine is underwater).
    pub bubble_phase: Fixed,
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
    /// 0x148–0x16B: Mirror of [`WeaponReleaseContext`] dwords `[1..=9]`
    /// (`worm_id`, `spawn_x`, `spawn_y`, `spawn_offset_x/y`,
    /// `ammo_per_turn`, `ammo_per_slot`, `_zero`, `delay`). Block-copied
    /// by the constructor; not yet referenced by the tick body in Rust.
    pub _unknown_148: [u8; 0x16C - 0x148],
    /// 0x16C: Placement-time fuse value in milliseconds, mirrored from
    /// [`WeaponReleaseContext::network_delay`]. Survives the per-frame
    /// fuse-timer countdown so the mine's countdown textbox can display
    /// the originally-selected fuse. Negative values mean "fuse rolled
    /// from replay log" — render falls back to `?` or the recorded
    /// value, gated by `_scheme_d934`.
    pub init_fuse_ms: i32,
    /// 0x170–0x18F: Mirror of [`WeaponFireParams`] dwords `[0..=7]`
    /// (`shot_count`, `spread`, `unknown_0x44`, `collision_radius`,
    /// `unknown_0x4c`, `unknown_0x50`, `unknown_0x54`, `unknown_0x58`).
    /// `WeaponFireParams[0..2,6]` are also mirrored in dedicated fields
    /// (`trigger_range`, `arm_delay`, `trigger_class_mask`, `damage`);
    /// the rest of the block is unreferenced by the tick body in Rust.
    pub _unknown_170: [u8; 0x190 - 0x170],
    /// 0x190: Animation phase counter; seeded from `(rng % 10) * 0x199A`
    /// and advanced each tick.
    pub _field_190: u32,
    /// 0x194: ProjectilePlay tracking index — sentinel `0xFFFFFFFF` until
    /// the mine registers itself with the active replay/projectile-play log.
    pub _field_194: u32,
    /// 0x198: This mine's countdown / state textbox, allocated by
    /// `MineEntity::ConstructPointers` via `DisplayGfx::ConstructTextbox`
    /// only when `world.is_headful != 0`; null in headless mode. Released
    /// at destruction time via [`Textbox::destroy`]. `MineEntity::Render`
    /// reads this slot to paint the per-mine countdown overlay.
    pub textbox_handle: *mut Textbox,
    /// 0x19C–0x1BB: Heap allocator only zeroes the first 0x19C bytes;
    /// nothing in the constructor or HandleMessage reads or writes this
    /// range.
    pub _unknown_19c: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<MineEntity>() == 0x1BC);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_MineEntityVtable!(MineEntity, base.base.vtable);
