use super::base::BaseEntity;
use super::game_task::WorldEntity;
use crate::FieldRegistry;

crate::define_addresses! {
    class "ArrowEntity" {
    }
}

/// ArrowEntity vtable — 12 slots. Extends WorldEntity vtable with arrow behavior.
///
/// Vtable at Ghidra 0x664198.
#[openwa_game::vtable(size = 12, va = 0x00664198, class = "ArrowEntity")]
pub struct ArrowEntityVtable {
    /// HandleMessage — processes arrow messages.
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(
        this: *mut ArrowEntity,
        sender: *mut BaseEntity,
        msg_type: u32,
        size: u32,
        data: *const u8,
    ),
    /// ProcessFrame — per-frame arrow update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut ArrowEntity, flags: u32),
}

/// Arrow/bullet projectile entity (Shotgun, Longbow).
///
/// Extends WorldEntity (0xFC bytes). Allocated size: 0x168 bytes.
/// Constructor zeros 0x148 of 0x168 bytes (first 0x20 not zeroed).
///
/// Inheritance: BaseEntity → WorldEntity → ArrowEntity. class_type = 0x0C (via ctor +0x20 = 12).
/// Constructor: 0x4FE130, stdcall(this, parent, fire_params, spawn_data), RET 0x10.
/// Vtable: 0x664198.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct ArrowEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94).
    pub base: WorldEntity<*const ArrowEntityVtable>,

    /// 0xFC–0x167: Arrow-specific fields. Layout unknown.
    pub _unknown_fc: [u8; 0x168 - 0xFC],
}

const _: () = assert!(core::mem::size_of::<ArrowEntity>() == 0x168);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_ArrowEntityVtable!(ArrowEntity, base.base.vtable);
