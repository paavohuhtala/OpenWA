/// DDNetGameWrapper — network game wrapper.
///
/// Constructor: DDNetGameWrapper__Constructor (0x56D1F0), stdcall(this) → DDNetGameWrapper*.
/// Size: 0x2C bytes.
///
/// Created by GameEngine__InitHardware (always, after DDGameWrapper).
/// Stored at GameSession+0xC0.
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct DDNetGameWrapper {
    pub _data: [u8; 0x2C],
}

const _: () = assert!(core::mem::size_of::<DDNetGameWrapper>() == 0x2C);
