/// DisplayGfx — main display/graphics subsystem container.
///
/// Constructor: DisplayGfx__Constructor (0x569C10), stdcall(this) → DisplayGfx*.
/// Size: 0x24E28 bytes.
///
/// Created by GameEngine__InitHardware in normal (non-headless) mode.
/// Stored at GameSession+0xAC.
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct DisplayGfx {
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 0x24E28 - 4],
}

const _: () = assert!(core::mem::size_of::<DisplayGfx>() == 0x24E28);
