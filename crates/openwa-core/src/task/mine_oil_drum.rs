use super::game_task::CGameTask;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CTaskMine" {
        /// CTaskMine vtable - mine entity
        vtable CTASK_MINE_VTABLE = 0x0066_43E8;
        ctor CTASK_MINE_CTOR = 0x0050_6660;
    }

    class "CTaskOilDrum" {
        /// CTaskOilDrum vtable - oil drum entity
        vtable CTASK_OILDRUM_VTABLE = 0x0066_4338;
        ctor CTASK_OILDRUM_CTOR = 0x0050_4AF0;
    }
}

/// Land mine entity task.
///
/// Extends CGameTask (0xFC bytes). Mines sit on the terrain and arm after
/// placement; they detonate on contact once armed.
///
/// Constructor: 0x506660 (stdcall).
/// Vtable: 0x6643E8. Class type byte: 0x08.
///
/// Source: Ghidra decompilation of 0x506660 (constructor) and
///         0x5072E0 (HandleMessage, msg 2/0x15/0x1C/0x4B branches).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskMine {
    /// 0x00–0xFB: CGameTask base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: CGameTask,
    /// 0xFC–0x10F: Unknown mine flags
    pub _unknown_fc: [u8; 0x14],
    /// 0x110: Object pool slot index (assigned from DDGame+0x3600 pool)
    pub slot_id: u32,
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

const _: () = assert!(core::mem::size_of::<CTaskMine>() == 0x128);

/// Exploding oil drum entity task.
///
/// Extends CGameTask (0xFC bytes). Oil drums roll on terrain and explode
/// when hit enough times (health decrements per impact).
///
/// Constructor: 0x504AF0 (thiscall).
/// Vtable: 0x664338. Class type byte: 0x1E.
///
/// Source: Ghidra decompilation of 0x504AF0 (constructor) and
///         0x5050B0 (HandleMessage, msg 2/0x1C branches).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskOilDrum {
    /// 0x00–0xFB: CGameTask base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: CGameTask,
    /// 0xFC: Triggered flag — set to 1 on first impact, starts smoke/fire
    pub triggered: u32,
    /// 0x100: Object pool slot index
    pub slot_id: u32,
    /// 0x104: Unknown
    pub _unknown_104: u32,
    /// 0x108: Health (starts at 0x32 = 50; decremented on damage)
    pub health: u32,
    /// 0x10C: Rolling animation counter (increments by 0x4000 per frame while moving)
    pub roll_counter: u32,
}

const _: () = assert!(core::mem::size_of::<CTaskOilDrum>() == 0x110);

impl CTaskOilDrum {
    /// Returns true if the drum is on fire (flag at CGameTask+0xB0, inside _unknown_98).
    ///
    /// # Safety
    /// `self` must be a valid, fully-constructed CTaskOilDrum.
    pub unsafe fn on_fire(&self) -> bool {
        let ptr = (self as *const CTaskOilDrum as *const u8).add(0xB0);
        *(ptr as *const u32) != 0
    }
}
