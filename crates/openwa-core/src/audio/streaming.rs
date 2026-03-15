/// StreamingAudio — streaming audio playback subsystem (music).
///
/// Constructor: FUN_0058BC10, usercall(ESI=this) + 2 stack params, RET 0x8.
/// Size: 0x354 bytes.
///
/// Created by GameEngine__InitHardware when speech is enabled
/// (GameInfo+0xDAA4 != 0) and DSSound initialized successfully.
/// Stored at GameSession+0xB4.
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct StreamingAudio {
    pub _data: [u8; 0x354],
}

const _: () = assert!(core::mem::size_of::<StreamingAudio>() == 0x354);
