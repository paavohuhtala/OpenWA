use super::base::BaseEntity;
use super::game_task::WorldEntity;
use crate::FieldRegistry;

crate::define_addresses! {
    class "MineEntity" {
        ctor MINE_ENTITY_CTOR = 0x00506660;
    }

    class "OilDrumEntity" {
        /// OilDrumEntity vtable - oil drum entity
        vtable OILDRUM_ENTITY_VTABLE = 0x00664338;
        ctor OILDRUM_ENTITY_CTOR = 0x00504AF0;
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

/// OilDrumEntity vtable — 12 slots. Extends WorldEntity vtable with oil drum behavior.
///
/// Vtable at Ghidra 0x664338.
#[openwa_game::vtable(size = 12, va = 0x00664338, class = "OilDrumEntity")]
pub struct OilDrumEntityVtable {
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
    /// ProcessFrame — per-frame oil drum update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut OilDrumEntity, flags: u32),
}

/// Land mine entity task.
///
/// Extends WorldEntity (0xFC bytes). Mines sit on the terrain and arm after
/// placement; they detonate on contact once armed.
///
/// Constructor: 0x506660 (stdcall).
/// Vtable: 0x6643E8. Class type byte: 0x08.
///
/// Source: Ghidra decompilation of 0x506660 (constructor) and
///         0x5072E0 (HandleMessage, msg 2/0x15/0x1C/0x4B branches).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MineEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: WorldEntity<*const MineEntityVtable>,
    /// 0xFC–0x10F: Unknown mine flags
    pub _unknown_fc: [u8; 0x14],
    /// 0x110: This mine's slot ID in `GameWorld.entity_activity_queue`.
    pub activity_rank_slot: u32,
    /// 0x114: Unknown
    pub _unknown_114: u32,
    /// 0x118: Fuse timer (signed i32).
    /// Negative = just placed / disarmed.
    /// 0 = armed (will trigger on contact).
    /// Positive = countdown ticks remaining.
    pub fuse_timer: i32,
    /// 0x11C: Unknown
    pub _unknown_11c: u32,
    /// 0x120–0x123: Unknown (init data param_3[0])
    pub _unknown_120: u32,
    /// 0x124: Owner team index (param_3[6]; -1 = no owner)
    pub owner_team: i32,
}

const _: () = assert!(core::mem::size_of::<MineEntity>() == 0x128);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_MineEntityVtable!(MineEntity, base.base.vtable);

/// Exploding oil drum entity task.
///
/// Extends WorldEntity (0xFC bytes). Oil drums roll on terrain and explode
/// when hit enough times (health decrements per impact).
///
/// Constructor: 0x504AF0 (thiscall).
/// Vtable: 0x664338. Class type byte: 0x1E.
///
/// Source: Ghidra decompilation of 0x504AF0 (constructor) and
///         0x5050B0 (HandleMessage, msg 2/0x1C branches).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct OilDrumEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: WorldEntity<*const OilDrumEntityVtable>,
    /// 0xFC: Triggered flag — set to 1 on first impact, starts smoke/fire
    pub triggered: u32,
    /// 0x100: This drum's slot ID in `GameWorld.entity_activity_queue`.
    pub activity_rank_slot: u32,
    /// 0x104: Unknown
    pub _unknown_104: u32,
    /// 0x108: Health (starts at 0x32 = 50; decremented on damage)
    pub health: u32,
    /// 0x10C: Rolling animation counter (increments by 0x4000 per frame while moving)
    pub roll_counter: u32,
}

const _: () = assert!(core::mem::size_of::<OilDrumEntity>() == 0x110);

// Generate typed vtable method wrappers: handle_message(), process_frame().
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
