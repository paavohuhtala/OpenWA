use super::game_task::CGameTask;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CTaskArrow" {
        /// CTaskArrow vtable — projectile entity for Shotgun/Longbow
        vtable CTASK_ARROW_VTABLE = 0x0066_4198;
    }
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
    pub base: CGameTask,

    /// 0xFC–0x167: Arrow-specific fields. Layout unknown.
    pub _unknown_fc: [u8; 0x168 - 0xFC],
}

const _: () = assert!(core::mem::size_of::<CTaskArrow>() == 0x168);
