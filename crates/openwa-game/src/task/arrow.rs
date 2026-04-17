use super::base::CTask;
use super::game_task::CGameTask;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CTaskArrow" {
        /// CTaskArrow vtable — projectile entity for Shotgun/Longbow
        vtable CTASK_ARROW_VTABLE = 0x0066_4198;
    }
}

/// CTaskArrow vtable — 12 slots. Extends CGameTask vtable with arrow behavior.
///
/// Vtable at Ghidra 0x664198.
#[openwa_game::vtable(size = 12, va = 0x0066_4198, class = "CTaskArrow")]
pub struct CTaskArrowVTable {
    /// HandleMessage — processes arrow messages.
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message:
        fn(this: *mut CTaskArrow, sender: *mut CTask, msg_type: u32, size: u32, data: *const u8),
    /// ProcessFrame — per-frame arrow update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut CTaskArrow, flags: u32),
}

/// Arrow/bullet projectile entity (Shotgun, Longbow).
///
/// Extends CGameTask (0xFC bytes). Allocated size: 0x168 bytes.
/// Constructor zeros 0x148 of 0x168 bytes (first 0x20 not zeroed).
///
/// Inheritance: CTask → CGameTask → CTaskArrow. class_type = 0x0C (via ctor +0x20 = 12).
/// Constructor: 0x4FE130, stdcall(this, parent, fire_params, spawn_data), RET 0x10.
/// Vtable: 0x664198.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskArrow {
    /// 0x00–0xFB: CGameTask base (pos at 0x84/0x88, speed at 0x90/0x94).
    pub base: CGameTask<*const CTaskArrowVTable>,

    /// 0xFC–0x167: Arrow-specific fields. Layout unknown.
    pub _unknown_fc: [u8; 0x168 - 0xFC],
}

const _: () = assert!(core::mem::size_of::<CTaskArrow>() == 0x168);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_CTaskArrowVTable!(CTaskArrow, base.base.vtable);
