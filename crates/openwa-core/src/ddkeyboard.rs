/// DDKeyboard — keyboard input subsystem.
///
/// Vtable: 0x66AEC8.
/// Destructor: DDKeyboard__Destructor (0x571B10).
/// PollKeyboardState (0x572290): drains WM_KEY messages, calls GetKeyboardState,
/// normalizes to both `key_state` (+0x11C) and `prev_state` (+0x21C) buffers.
///
/// PARTIAL: Only confirmed fields. Minimum size estimated from known offsets.
#[repr(C)]
pub struct DDKeyboard {
    /// 0x000: Vtable pointer (0x66AEC8)
    pub vtable: *mut u8,
    /// 0x004-0x11B: Unknown
    pub _unknown_004: [u8; 0x118],
    /// 0x11C: Current key state buffer (256 bytes).
    /// Populated by PollKeyboardState via GetKeyboardState.
    pub key_state: [u8; 256],
    /// 0x21C: Previous key state buffer (256 bytes).
    /// Copied from key_state at start of each poll cycle.
    pub prev_state: [u8; 256],
}
