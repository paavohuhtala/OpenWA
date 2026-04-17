use crate::engine::ddgame::DDGame;

/// NetBridge — lightweight adapter between DDGame and the network layer (0x2C bytes).
///
/// Only created for online games (`game_version == -2`). Allocated during
/// `DDGame__Constructor` and stored at `DDGameWrapper+0x48C`.
/// Also linked into the network context object (constructor param 7, via ECX)
/// at offset +0x18.
///
/// Not to be confused with `DDNetGameWrapper` (same size, different purpose) —
/// that one is constructed by `GameEngine__InitHardware` and stored at
/// `GameSession+0xC0`.
#[repr(C)]
pub struct NetBridge {
    /// 0x00: Back-pointer to the owning DDGame instance.
    pub ddgame: *mut DDGame,
    /// 0x04-0x27: Unknown (zero-filled on construction).
    pub _unknown_04: [u8; 0x24],
    /// 0x28: Network config byte 1 (from GameInfo+0xD944).
    pub net_config_1: u8,
    /// 0x29: Network config byte 2 (from GameInfo+0xD946).
    pub net_config_2: u8,
    /// 0x2A-0x2B: Padding (zero-filled).
    pub _pad_2a: [u8; 2],
}

const _: () = assert!(core::mem::size_of::<NetBridge>() == 0x2C);
