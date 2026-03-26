use super::base::CTask;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CTaskFilter" {
        /// CTaskFilter vtable - role unclear; 4 instances in a 2-team 3-worm game
        vtable CTASK_FILTER_VTABLE = 0x0066_9DAC;
        ctor CTASK_FILTER_CTOR = 0x0054_F3D0;
    }
}

/// CTaskFilter vtable — 12 slots. Extends CTask base (8 slots) with filter behavior.
///
/// Vtable at Ghidra 0x669DAC. Slot 2 (HandleMessage) checks the subscription
/// table before forwarding messages to children.
#[openwa_core::vtable(size = 12, va = 0x0066_9DAC, class = "CTaskFilter")]
pub struct CTaskFilterVTable {
    /// HandleMessage — checks subscription table, forwards matching messages.
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(this: *mut CTaskFilter, sender: *mut CTask, msg_type: u32, size: u32, data: *const u8),
    /// ProcessFrame — per-frame filter update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut CTaskFilter, flags: u32),
}

/// Message-subscription filter task — routes messages selectively to child tasks.
///
/// CTaskFilter is a CTask subclass that overrides HandleMessage to only forward
/// messages whose type is marked in a 100-entry boolean subscription table. Each
/// CTaskFilter instance subscribes to a specific set of message IDs at construction
/// time; all other messages are silently dropped before reaching the subtree.
///
/// **Role in the task tree**: CTaskTeam creates multiple CTaskFilter children per
/// team during construction. Each filter represents a different event-routing path
/// (e.g., movement, UI, game-flow, weather). Messages from CTaskTurnGame propagate
/// down through these filters, which gate access to their subtrees.
///
/// **Allocation size**: 0xB4 bytes (via operator new in factory functions).
///
/// **Constructor**: `CTaskFilter__Constructor` (0x54F3D0, thiscall):
/// - `init_val_1c`: stored at CTask+0x1C (role unknown)
/// - `parent_task`: parent in the task tree (determines shared_data)
///
/// **Key vtable methods** (vtable at 0x669DAC):
/// - [2] `CTaskFilter__HandleMessage` (0x54F4A0): checks subscription table, forwards
///   only if `msg_type < 100 && subscription_table[msg_type] != 0`
/// - [7] `CTaskFilter__SubscribeAll` (0x54F390): sets all 100 entries to 1
/// - [8] `CTaskFilter__Subscribe` (0x54F370): sets `subscription_table[msg_id] = 1`
///
/// **Four factory functions** (all called by `CTaskTeam__Constructor_Maybe` 0x550E70):
/// - `FUN_00552030`: subscribes to messages 0, 1, 3, 5
/// - `FUN_005520D0`: subscribes to messages 0, 1, 2, 3, 0x15, 0x18, 0x1C
/// - `FUN_00552190`: subscribes to messages 0, 1, 2, 3, 5, 0x15, 0x17, 0x1C, 0x2C–0x2E, 0x4B,
///   and optionally 0x0E (if `GameInfo+0xD778 < -1`)
/// - `CTaskTeam__CreateWeatherFilter` (0x552960): subscribes to 1, 2, 3, 0x54, then
///   spawns `CTaskCloud` children using randomised positions (only if `DDGame+0x777C == 0`)
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskFilter {
    /// 0x00–0x2F: CTask base.
    ///
    /// Notable base fields set by CTaskFilter__Constructor:
    /// - CTask+0x18 (`_unknown_18`): set to 0
    /// - CTask+0x1C (`_unknown_1c`): set to `init_val_1c` constructor param
    /// - CTask+0x20 (`_unknown_20`): set to 7 (task type / mode constant)
    pub base: CTask<*const CTaskFilterVTable>,
    /// 0x30–0x93: Boolean subscription table, indexed by message type ID (0–99).
    ///
    /// `subscription_table[id] != 0` means this filter will forward messages of
    /// that type. Cleared to 0 at construction, then populated by Subscribe/SubscribeAll
    /// calls. Max 100 distinct message IDs (IDs >= 100 always pass through).
    pub subscription_table: [u8; 100],
    /// 0x94–0xB3: Unknown (present in 0xB4-byte allocation; not set by constructor).
    pub _unknown_94: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<CTaskFilter>() == 0xB4);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_CTaskFilterVTable!(CTaskFilter, base.vtable);
