/// GameStats — headless mode display stub.
///
/// Constructor: GameStats__Constructor (0x522DB0), stdcall(this) → GameStats*.
/// Vtable overlay: 0x66A0F8 (applied after constructor returns).
/// Size: 0x3560 bytes.
///
/// Created by GameEngine__InitHardware in headless mode (GameInfo+0xF914 != 0)
/// instead of DisplayGfx. Stored in the same GameSession+0xAC slot.
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct GameStats {
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 0x3560 - 4],
}

const _: () = assert!(core::mem::size_of::<GameStats>() == 0x3560);
