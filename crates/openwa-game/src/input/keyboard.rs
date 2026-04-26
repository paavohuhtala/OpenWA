use crate::address::va;
use crate::engine::game_session::get_game_session;
use crate::rebase::rb;
use crate::wa_alloc::wa_free;

/// Keyboard vtable (0x66AEC8). 19 slots in memory; the slots beyond the
/// concrete keyboard methods (9–18) are MSVC base-class destructor + small
/// `return 0;` stubs left over from the C++ class layout.
#[openwa_game::vtable(size = 19, va = 0x0066AEC8, class = "Keyboard")]
pub struct KeyboardVtable {
    /// Slot 0: scalar deleting destructor (0x571B10).
    #[slot(0)]
    pub destructor: fn(this: *mut Keyboard, flags: u32) -> *mut Keyboard,
    /// Slot 1: Keyboard::IsActionPressed (0x572210). `usercall(EAX=action,
    /// ESI=this, EDI=0)` → CheckAction. Edge-triggered (key just-pressed).
    #[slot(1)]
    pub is_action_pressed: fn(this: *mut Keyboard, code: u32) -> i32,
    /// Slot 2: Keyboard::IsActionActive (0x572230). EDI=1 → CheckAction.
    #[slot(2)]
    pub is_action_active: fn(this: *mut Keyboard, code: u32) -> i32,
    /// Slot 3: Keyboard::IsActionActive2 (0x572250). EDI=2 → CheckAction.
    /// Level-triggered alternative; returns nonzero while held.
    #[slot(3)]
    pub is_action_active2: fn(this: *mut Keyboard, code: u32) -> i32,
    /// Slot 4: Keyboard::IsActionHeld (0x572270). EDI=-1 → CheckAction.
    #[slot(4)]
    pub is_action_held: fn(this: *mut Keyboard, code: u32) -> i32,
    /// Slot 5: Keyboard::ReadInputRingBuffer (0x571B30). Pops one byte from
    /// the 256-entry ASCII ring buffer; returns 0 when empty.
    #[slot(5)]
    pub read_input_ring_buffer: fn(this: *mut Keyboard) -> u8,
    /// Slot 6: shared `WorldEntity__vt19` ret stub (0x4AA060). No-op on the
    /// stock vtable; kept as a hook point (WormKit etc. may override).
    /// Called each frame from StepFrame when `GameWorld.is_headful != 0`.
    #[slot(6)]
    pub slot_06_noop: fn(this: *mut Keyboard),
    /// Slot 7: Keyboard::VFunc7 (0x5723D0). Calls vtable[8] (`AlertUser`) on
    /// `this` with `flash = (*game_info_input_ptr == 0)`, `beep_kind = 1`.
    /// I.e. flash the window when the input-state slot is currently zero.
    #[slot(7)]
    pub vfunc7: fn(this: *mut Keyboard),
    /// Slot 8: Keyboard::AlertUser (0x572320). Notifies the user when
    /// the game window is not foreground — `MessageBeep(beep_kind)` plus
    /// `FlashWindow(g_FrontendHwnd)` if `flash != 0`. Called from the
    /// post-loop cleanup in `GameSession::Run`.
    #[slot(8)]
    pub alert_user: fn(this: *mut Keyboard, flash: u8, beep_kind: i32),
}

bind_KeyboardVtable!(Keyboard, vtable);

/// Keyboard — keyboard input subsystem.
///
/// Vtable: 0x66AEC8. Size: 0x33C bytes. Inline construction in
/// `GameEngine__InitHardware` (no native C++ ctor):
///   - vtable = 0x66AEC8
///   - game_info_input_ptr = &GameInfo+0xF918
///   - _field_008 = 1
///   - key_state / prev_state zeroed (each 0x100 bytes)
///   - ring_head / ring_tail = 0
#[repr(C)]
pub struct Keyboard {
    /// 0x000: Vtable pointer (0x66AEC8)
    pub vtable: *const KeyboardVtable,
    /// 0x004: Pointer into GameInfo+0xF918 (shared input state location).
    /// Stores the ADDRESS of GameInfo+0xF918, not its value.
    pub game_info_input_ptr: u32,
    /// 0x008: Init flag (set to 1 by inline ctor)
    pub _field_008: u32,
    /// 0x00C-0x013: Unknown
    pub _unknown_00c: [u8; 8],
    /// 0x014: ASCII ring-buffer head (write index, mod 0x100). Producer
    /// (likely the WM_CHAR handler) advances this; ReadInputRingBuffer is
    /// the consumer.
    pub ring_head: u32,
    /// 0x018: ASCII ring-buffer tail (read index, mod 0x100).
    pub ring_tail: u32,
    /// 0x01C: ASCII ring-buffer storage (256 bytes, indexed by `ring_tail`).
    pub ring_buffer: [u8; 0x100],
    /// 0x11C: Current key state buffer (256 bytes, indexed by VK_*).
    /// Populated by `Keyboard__PollState` via `GetKeyboardState`; each entry
    /// is normalized to 0/1 (`high bit set` → 1).
    pub key_state: [u8; 256],
    /// 0x21C: Previous key state buffer (256 bytes). Used by `CheckKeyState`
    /// for edge detection — set to nonzero once an edge fires so the same
    /// keypress isn't re-reported.
    pub prev_state: [u8; 256],
    /// 0x31C: Unknown trailing fields (32 bytes)
    pub _unknown_31c: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<Keyboard>() == 0x33C);

impl Keyboard {
    /// Clear both key state buffers.
    ///
    /// Port of Keyboard__ClearKeyStates (0x5722F0, usercall EAX=this, plain RET).
    pub fn clear_key_states(&mut self) {
        self.key_state.fill(0);
        self.prev_state.fill(0);
    }

    /// Poll keyboard state via WA's `Keyboard__PollState` (0x572290).
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn poll(&mut self) {
        unsafe {
            let poll_fn: unsafe extern "stdcall" fn(*mut Self) =
                core::mem::transmute(rb(va::KEYBOARD_POLL_STATE) as usize);
            poll_fn(self);
        }
    }

    /// Create a new Keyboard with inline construction (no native C++ ctor).
    ///
    /// # Safety
    /// `vtable_addr` must be a valid rebased vtable pointer.
    /// `input_ptr` must be the address of `GameInfo.input_state_f918`.
    pub unsafe fn new(vtable_addr: u32, input_ptr: u32) -> Self {
        Self {
            vtable: vtable_addr as *const KeyboardVtable,
            game_info_input_ptr: input_ptr,
            _field_008: 1,
            _unknown_00c: [0; 8],
            ring_head: 0,
            ring_tail: 0,
            ring_buffer: [0; 0x100],
            key_state: [0; 256],
            prev_state: [0; 256],
            _unknown_31c: [0; 0x20],
        }
    }
}

// ─── Vtable replacements ─────────────────────────────────────────────────────
//
// Free `unsafe extern "thiscall" fn` impls suitable for `vtable_replace!`.

/// Port of `Keyboard__Destructor` (0x571B10).
///
/// `__thiscall(this=ECX, flags=stack)`, `RET 0x4`. Returns `this`. Sets
/// `this->vtable = vtable + 0x24` (slot 9 — the MSVC base-class destructor
/// stub) before optionally `free`-ing. The vtable rewrite has no observable
/// effect when `flags & 1` (free immediately follows), but we replicate it
/// for the rare partial-destruction path.
pub unsafe extern "thiscall" fn keyboard_destructor(
    this: *mut Keyboard,
    flags: u32,
) -> *mut Keyboard {
    unsafe {
        let mid_vtable = (rb(va::KEYBOARD_VTABLE) + 0x24) as *const KeyboardVtable;
        (*this).vtable = mid_vtable;
        if flags & 1 != 0 {
            wa_free(this);
        }
        this
    }
}

/// Port of `Keyboard__ReadInputRingBuffer` (0x571B30).
///
/// `__thiscall(this=ECX) -> u8`, plain `RET`. Pops one byte from the 256-entry
/// ASCII ring buffer at `+0x1C`; returns 0 when `head == tail` (empty).
pub unsafe extern "thiscall" fn keyboard_read_input_ring_buffer(this: *mut Keyboard) -> u8 {
    unsafe {
        let head = (*this).ring_head;
        let tail = (*this).ring_tail;
        if head == tail {
            return 0;
        }
        let value = (*this).ring_buffer[tail as usize];
        (*this).ring_tail = (tail + 1) & 0xFF;
        value
    }
}

/// Port of `Keyboard__AlertUser` (0x572320).
///
/// `__thiscall(this=ECX, flash=stack[u8], beep_kind=stack[i32])`, `RET 0x8`.
/// Notifies the user when WA isn't the foreground window: optional MessageBeep
/// + optional window flash. No-op when the foreground is already the in-game
/// window or the menu window.
pub unsafe extern "thiscall" fn keyboard_alert_user(
    _this: *mut Keyboard,
    flash: u8,
    beep_kind: i32,
) {
    unsafe {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            FLASHW_TIMERNOFG, FLASHW_TRAY, FLASHWINFO, FlashWindow, GetForegroundWindow,
            MB_ICONINFORMATION, MB_ICONWARNING,
        };

        let foreground = GetForegroundWindow();
        let session = get_game_session();
        if !session.is_null() && foreground as u32 == (*session).hwnd {
            return;
        }
        let frontend_hwnd = *(rb(va::G_FRONTEND_HWND) as *const HWND);
        if foreground == frontend_hwnd {
            return;
        }

        match beep_kind {
            1 => {
                MessageBeep(MB_ICONINFORMATION);
            }
            2 => {
                MessageBeep(MB_ICONWARNING);
            }
            _ => {}
        }

        if flash == 0 {
            return;
        }

        // WA resolves `FlashWindowEx` at startup via `GetProcAddress` and stores
        // it at G_FLASH_WINDOW_EX_FN; calling through that slot honors WormKit
        // overrides and matches the original behavior on Win9x (slot null →
        // FlashWindow fallback path).
        let flash_ex_ptr = *(rb(va::G_FLASH_WINDOW_EX_FN) as *const usize);
        if flash_ex_ptr == 0 {
            FlashWindow(frontend_hwnd, 1);
            *(rb(va::G_WINDOW_FLASHING) as *mut u32) = 1;
        } else {
            let flash_ex: unsafe extern "system" fn(*const FLASHWINFO) -> i32 =
                core::mem::transmute(flash_ex_ptr);
            let info = FLASHWINFO {
                cbSize: core::mem::size_of::<FLASHWINFO>() as u32,
                hwnd: frontend_hwnd,
                dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
                uCount: 15,
                dwTimeout: 0,
            };
            flash_ex(&info);
        }
    }
}

/// Port of `Keyboard__VFunc7` (0x5723D0).
///
/// `__thiscall(this=ECX)`, plain `RET`. Convenience helper that calls vtable
/// slot 8 (`AlertUser`) on `this` with `flash = (*game_info_input_ptr == 0)`
/// and `beep_kind = 1`. Effectively: "flash + info-beep when the input-state
/// slot at `GameInfo+0xF918` is currently zero."
pub unsafe extern "thiscall" fn keyboard_vfunc7(this: *mut Keyboard) {
    unsafe {
        let input_state_ptr = (*this).game_info_input_ptr as *const u32;
        let flash = (*input_state_ptr == 0) as u8;
        ((*(*this).vtable).alert_user)(this, flash, 1);
    }
}
