/// DDKeyboard vtable (0x66AEC8, 8 slots).
#[openwa_game::vtable(size = 8, va = 0x0066_AEC8, class = "DDKeyboard")]
pub struct DDKeyboardVtable {
    /// scalar deleting destructor (0x571B10).
    #[slot(0)]
    pub destructor: fn(this: *mut DDKeyboard, flags: u32) -> *mut DDKeyboard,
    /// DDKeyboard::IsActionPressed (0x572210) — returns the edge-triggered state
    /// for the action code. Wraps `CheckAction`, which compares `key_state`
    /// against `prev_state`.
    #[slot(1)]
    pub is_action_pressed: fn(this: *mut DDKeyboard, code: u32) -> i32,
    /// DDKeyboard::IsActionActive2 (0x572250) — level-triggered; returns
    /// nonzero while the bound key/combo is held.
    #[slot(3)]
    pub is_action_active: fn(this: *mut DDKeyboard, code: u32) -> i32,
    /// Slot 6: shared `CGameTask__vt19` ret stub (0x4AA060). No-op on the
    /// stock vtable; kept as a hook point (WormKit etc. may override).
    /// Called each frame from StepFrame when `DDGame.is_headful != 0`.
    #[slot(6)]
    pub slot_06_noop: fn(this: *mut DDKeyboard),
}

bind_DDKeyboardVtable!(DDKeyboard, vtable);

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
    pub vtable: *const DDKeyboardVtable,
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
    /// Clear both key state buffers.
    ///
    /// Port of DDKeyboard__ClearKeyStates (0x5722F0, usercall EAX=this, plain RET).
    /// Zeroes `key_state` (+0x11C) and `prev_state` (+0x21C), each 256 bytes.
    pub fn clear_key_states(&mut self) {
        self.key_state.fill(0);
        self.prev_state.fill(0);
    }

    /// Poll keyboard state via WA's PollKeyboardState (0x572290).
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn poll(&mut self) {
        unsafe {
            use crate::address::va;
            use crate::rebase::rb;
            let poll_fn: unsafe extern "stdcall" fn(*mut Self) =
                core::mem::transmute(rb(va::DDKEYBOARD_POLL_KEYBOARD_STATE) as usize);
            poll_fn(self);
        }
    }

    /// Create a new DDKeyboard with inline construction (no native C++ ctor).
    ///
    /// All fields are zero-initialized, then known fields are set.
    ///
    /// # Safety
    /// `vtable_addr` must be a valid rebased vtable pointer.
    /// `input_ptr` must be the address of `GameInfo.input_state_f918`.
    pub unsafe fn new(vtable_addr: u32, input_ptr: u32) -> Self {
        Self {
            vtable: vtable_addr as *const DDKeyboardVtable,
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
