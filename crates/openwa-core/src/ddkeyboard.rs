/// DDKeyboard — keyboard input subsystem.
///
/// Vtable: 0x66AEC8.
/// Destructor: DDKeyboard__Destructor (0x571B10).
/// PollKeyboardState (0x572290): drains WM_KEY messages, calls GetKeyboardState,
/// normalizes to both `key_state` (+0x11C) and `prev_state` (+0x21C) buffers.
///
/// Size: 0x33C bytes.
/// Inline construction in GameEngine__InitHardware:
///   - vtable = 0x66AEC8
///   - dinput_device = GameInfo+0xF918
///   - _field_008 = 1
///   - key_state zeroed (0x100 bytes)
///   - prev_state zeroed (0x100 bytes)
///   - _field_014 = 0, _field_018 = 0
#[repr(C)]
pub struct DDKeyboard {
    /// 0x000: Vtable pointer (0x66AEC8)
    pub vtable: *mut u8,
    /// 0x004: Pointer into GameInfo+0xF918 (shared input state location).
    /// Stores the ADDRESS of GameInfo+0xF918, not its value.
    pub game_info_input_ptr: u32,
    /// 0x008: Init flag (set to 1)
    pub _field_008: u32,
    /// 0x00C-0x013: Unknown
    pub _unknown_00c: [u8; 8],
    /// 0x014: Cleared to 0 during construction
    pub _field_014: u32,
    /// 0x018: Cleared to 0 during construction
    pub _field_018: u32,
    /// 0x01C-0x11B: Unknown
    pub _unknown_01c: [u8; 0x100],
    /// 0x11C: Current key state buffer (256 bytes).
    /// Populated by PollKeyboardState via GetKeyboardState.
    pub key_state: [u8; 256],
    /// 0x21C: Previous key state buffer (256 bytes).
    /// Copied from key_state at start of each poll cycle.
    pub prev_state: [u8; 256],
    /// 0x31C: Unknown trailing fields
    pub _unknown_31c: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<DDKeyboard>() == 0x33C);

impl DDKeyboard {
    /// Create a new DDKeyboard with inline construction (no native C++ ctor).
    ///
    /// All fields are zero-initialized, then known fields are set.
    ///
    /// # Safety
    /// `vtable_addr` must be a valid rebased vtable pointer.
    /// `input_ptr` must be the address of `GameInfo.input_state_f918`.
    pub unsafe fn new(vtable_addr: u32, input_ptr: u32) -> Self {
        Self {
            vtable: vtable_addr as *mut u8,
            game_info_input_ptr: input_ptr,
            _field_008: 1,
            _unknown_00c: [0; 8],
            _field_014: 0,
            _field_018: 0,
            _unknown_01c: [0; 0x100],
            key_state: [0; 256],
            prev_state: [0; 256],
            _unknown_31c: [0; 0x20],
        }
    }
}
