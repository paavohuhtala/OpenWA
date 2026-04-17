/// GameTimer — timing subsystem.
///
/// Constructor: FUN_0053E950, usercall(ESI=this, EAX=init_val), plain RET.
/// Size: 0x30 bytes (header). Constructor allocates 2 × 0x20E0 internal buffers.
///
/// Created by GameEngine__InitHardware (always).
/// Stored at GameSession+0xBC.
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct GameTimer {
    pub _data: [u8; 0x30],
}

const _: () = assert!(core::mem::size_of::<GameTimer>() == 0x30);
