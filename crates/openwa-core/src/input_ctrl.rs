/// InputCtrl — input controller subsystem.
///
/// Initializer: FUN_0058C0D0, usercall(ESI=this) + stdcall(4 params), RET 0x10.
/// Vtable: 0x66B3FC.
/// Size: 0x1800 bytes.
///
/// Created by GameEngine__InitHardware when param4 != 0.
/// Stored at GameSession+0xB8.
///
/// OPAQUE: Internal layout not yet mapped beyond vtable and a few known fields.
#[repr(C)]
pub struct InputCtrl {
    /// 0x000: Vtable pointer (0x66B3FC)
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 0xD74 - 4],
    /// 0xD74: Set to 0x3F9 during inline construction.
    pub _field_d74: u32,
    pub _unknown_d78: [u8; 0x1800 - 0xD78],
}

const _: () = assert!(core::mem::size_of::<InputCtrl>() == 0x1800);
