use crate::address::va;
use crate::engine::game_info::GameInfo;
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
    /// 0x00C-0x00F: Unknown
    pub _unknown_00c: [u8; 4],
    /// 0x010: Sticky scratch latch used exclusively by `CheckAction` case 0x0D
    /// (HOME). Set to 1 when HOME fires while CTRL is held — combined with the
    /// re-armed `prev_state[VK_HOME]=2`, this makes HOME re-fire every frame
    /// while CTRL is held. Cleared on any frame where HOME is not pressed or
    /// CTRL is released.
    pub home_ctrl_latch: u32,
    /// 0x014: ASCII ring-buffer head (write index, mod 0x100). Advanced by
    /// `GameSession::WindowProc`'s WM_CHAR (0x102) case; consumed by
    /// `Keyboard::ReadInputRingBuffer` (vtable slot 5). Drops when full
    /// (`new_head == ring_tail`) and on null bytes.
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
}

/// cdecl-callable impl behind the EAX-passing usercall hook for
/// `Keyboard__ClearKeyStates` (0x005722F0). The trampoline that captures
/// `this` from EAX lives in `replacements/keyboard.rs`.
pub unsafe extern "cdecl" fn keyboard_clear_key_states_impl(this: *mut Keyboard) {
    unsafe { (*this).clear_key_states() }
}

impl Keyboard {
    /// Poll keyboard state — wrapper around the now-Rust `keyboard_poll_state`
    /// (the implementation that's also installed as the WA-side replacement).
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn poll(&mut self) {
        unsafe { keyboard_poll_state(self) }
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
            _unknown_00c: [0; 4],
            home_ctrl_latch: 0,
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

/// Port of `Keyboard__PollState` (0x00572290).
///
/// `__stdcall(this)`, `RET 0x4`. Drains all pending keyboard messages from
/// the queue (`PeekMessageA` over `WM_KEYFIRST..=WM_KEYLAST + 1` with
/// `PM_REMOVE`), snapshots the global key state into `key_state[256]`, then
/// normalizes: each byte's high bit (Win32's "key down" indicator) becomes
/// `1`, anything else `0`. The same normalized value is also written to
/// `prev_state[256]` — `CheckKeyState` interprets a nonzero `prev_state` as
/// "edge already fired" and uses this as its baseline for the next poll
/// cycle.
pub unsafe extern "stdcall" fn keyboard_poll_state(this: *mut Keyboard) {
    unsafe {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardState;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MSG, PM_REMOVE, PeekMessageA, WM_KEYFIRST, WM_KEYLAST,
        };

        // `GetKeyboardState` reports key-down via the byte's high bit. Win32
        // doesn't ship a name for the bit; 0x80 is canonical.
        const KEY_DOWN_BIT: u8 = 0x80;

        // Drain `WM_KEYFIRST..=WM_KEYLAST + 1` with `PM_REMOVE`. WA reaches
        // one past `WM_KEYLAST` (0x108) — preserved verbatim; Win32 treats
        // the extra slot as a no-op since no message ID maps to 0x109.
        let mut msg: MSG = core::mem::zeroed();
        while PeekMessageA(
            &mut msg,
            core::ptr::null_mut(),
            WM_KEYFIRST,
            WM_KEYLAST + 1,
            PM_REMOVE,
        ) != 0
        {}

        GetKeyboardState((*this).key_state.as_mut_ptr());

        for i in 0..(*this).key_state.len() {
            let pressed = (*this).key_state[i] & KEY_DOWN_BIT != 0;
            let byte = pressed as u8;
            (*this).key_state[i] = byte;
            (*this).prev_state[i] = byte;
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

/// Port of `Keyboard__AcquireInput` (0x00572500).
///
/// `__usercall(ESI=esi_flag, [ESP+4]=param_1) -> void, RET 0x4`. Operates on
/// the *global* `g_GameSession` — does NOT take `this` despite living on the
/// Keyboard class (name is historical). Called from the OnSYSCOMMAND focus-
/// restore path; re-acquires input ownership after the WA window regains
/// foreground.
///
/// `esi_flag` selects the "full re-acquire" branch (cursor recapture + focus);
/// `param_1` selects whether to also restore renderer dimensions.
///
/// Caller is `keyboard_acquire_input_trampoline` — a naked shim that captures
/// ESI before calling this cdecl impl.
pub unsafe extern "cdecl" fn keyboard_acquire_input_impl(esi_flag: u32, param_1: u32) {
    unsafe {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let session = get_game_session();
        if session.is_null() || (*session).flag_5c == 0 {
            return;
        }

        if esi_flag != 0 {
            GetCursorPos(&raw mut (*session).cursor_initial);
        }

        (*session).mouse_acquired = esi_flag;

        // Bridge: FrontendDialog::UpdateCursor(g_InGameFrontendDialog) — stdcall.
        let update_cursor: unsafe extern "stdcall" fn(u32) =
            core::mem::transmute(rb(va::FRONTEND_DIALOG_UPDATE_CURSOR) as usize);
        update_cursor(rb(va::G_INGAME_FRONTEND_DIALOG));

        if esi_flag != 0 {
            crate::input::mouse::cursor_clip_and_recenter();
        }

        (*session).flag_60 = 1;
        (*session).cursor_recenter_request = esi_flag;

        if param_1 == 0 {
            // g_RenderContext->vtable[10] (renderer_restore_dims) — fastcall
            // returning a FastcallResult. The decompile uses `LEA EDX, [ESP+0x4]`
            // for the result-buffer pointer; we pass a stack local instead.
            use crate::render::display::FastcallResult;
            use crate::render::display::context::RenderContext;
            let ctx = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
            if !ctx.is_null() {
                let mut result: FastcallResult = core::mem::zeroed();
                ((*(*ctx).vtable).renderer_restore_dims)(ctx, &mut result);
            }
        }

        // Now-Rust call: poll the keyboard.
        let kb = (*session).keyboard;
        if !kb.is_null() {
            keyboard_poll_state(kb);
        }

        let display = (*session).display;
        if display.is_null() {
            return;
        }

        // Bridge: Display__RestoreSurfaces_Maybe(display) -> u32, stdcall.
        let restore_surfaces: unsafe extern "stdcall" fn(*mut u8) -> u32 =
            core::mem::transmute(rb(va::DISPLAY_RESTORE_SURFACES) as usize);
        if restore_surfaces(display) == 0 {
            return;
        }

        if esi_flag != 0 {
            let frontend_hwnd = *(rb(va::G_FRONTEND_HWND) as *const HWND);
            SetFocus(frontend_hwnd);
        }

        (*session).flag_5c = 0;
    }
}

/// Naked trampoline matching WA's `__usercall(ESI=esi_flag, [ESP+4]=param_1)`.
/// Captures ESI, forwards both args to `keyboard_acquire_input_impl`, returns
/// via `RET 0x4` to clean the caller's single stack arg.
#[unsafe(naked)]
pub unsafe extern "C" fn keyboard_acquire_input() {
    core::arch::naked_asm!(
        // Entry: [esp+0]=ret_addr, [esp+4]=param_1, ESI=esi_flag.
        "pushl %esi",                  // [esp+0]=esi, [esp+4]=ret, [esp+8]=p1
        "movl 8(%esp), %eax",          // EAX = param_1 (preserve caller's stack)
        "pushl %eax",                  // arg2 = param_1
        "pushl %esi",                  // arg1 = esi_flag
        "calll {impl_fn}",
        "addl $8, %esp",               // clean cdecl args
        "popl %esi",                   // restore caller's ESI
        "retl $4",                     // stdcall-style: clean caller's 1 stack arg
        impl_fn = sym keyboard_acquire_input_impl,
        options(att_syntax),
    );
}

// ─── CheckAction / CheckKeyState ─────────────────────────────────────────────
//
// Port of Keyboard::CheckAction (0x00571BA0) and Keyboard::CheckKeyState
// (0x00571B50). Once all four IsAction* vtable slots (1/2/3/4) are replaced
// with the Rust shims at the bottom of this section, CheckAction becomes
// unreachable WA-side and the original 0x00571BA0 can be trapped.

// Win32 VK constants used inline. Win32's KeyboardAndMouse module exports
// `VIRTUAL_KEY` typedefs but as `u16`; we want raw `u8` indices into
// `key_state` / `prev_state` so we name them locally. Values from WinUser.h.
const VK_BACK: u8 = 0x08;
const VK_TAB: u8 = 0x09;
const VK_CLEAR: u8 = 0x0C;
const VK_RETURN: u8 = 0x0D;
const VK_SHIFT_VK: u8 = 0x10;
const VK_CONTROL_VK: u8 = 0x11;
const VK_MENU_VK: u8 = 0x12; // Alt
const VK_ESCAPE: u8 = 0x1B;
const VK_SPACE: u8 = 0x20;
const VK_PRIOR: u8 = 0x21; // PageUp
const VK_NEXT: u8 = 0x22; // PageDown
const VK_END: u8 = 0x23;
const VK_HOME: u8 = 0x24;
const VK_LEFT: u8 = 0x25;
const VK_UP: u8 = 0x26;
const VK_RIGHT: u8 = 0x27;
const VK_DOWN: u8 = 0x28;
const VK_INSERT: u8 = 0x2D;
const VK_DELETE: u8 = 0x2E;
const VK_M: u8 = 0x4D;
const VK_R: u8 = 0x52;
const VK_S: u8 = 0x53;
const VK_T: u8 = 0x54;
const VK_V: u8 = 0x56;
const VK_NUMPAD_ADD: u8 = 0x6B;
const VK_NUMPAD_SUBTRACT: u8 = 0x6D;
const VK_SCROLL: u8 = 0x91;
const VK_OEM_PLUS: u8 = 0xBB;
const VK_OEM_MINUS: u8 = 0xBD;

/// Port of `Keyboard__CheckKeyState` (0x00571B50).
///
/// `__usercall(EAX=key, EDX=mode, [ESP+4]=this) -> bool`.
///
/// Mode is a packed selector for edge/level semantics (see IsAction* wrappers):
/// - `mode == 0` (`IsActionPressed`): edge — returns true on first read while
///   `key_state[k]!=0`, but doesn't write the latch (leaves `prev_state[k]=0`),
///   so subsequent same-mode probes can still see the press.
/// - `mode == 1/2` (`IsActionActive`/`Active2`): edge — sets `prev_state[k]=mode`
///   so the next call returns false until released.
/// - `mode == -1` (`IsActionHeld`): level — pure read of `prev_state[k]`.
///
/// Note: the writeback `prev_state[k] = mode as u8` stores the mode value
/// itself; `mode = 0` thus leaves the slot unset, while `mode = 1/2` arm it
/// distinctly. CheckAction's case 0x0D and 0x36 take advantage of these
/// numeric values.
#[inline]
unsafe fn check_key_state(this: *mut Keyboard, key: u8, mode: i32) -> bool {
    unsafe {
        let k = key as usize;
        if mode < 0 {
            return (*this).prev_state[k] != 0;
        }
        if (*this).prev_state[k] != 0 {
            return false;
        }
        let pressed = (*this).key_state[k] != 0;
        if pressed {
            (*this).prev_state[k] = mode as u8;
        }
        pressed
    }
}

/// Faithful enum of action codes recognized by `Keyboard::CheckAction`.
///
/// Variants are named `A<hex>` to preserve WA's action numbering exactly —
/// no semantic interpretation yet. Each doc comment summarizes which key the
/// action probes and any modifier guards it applies. Will be refactored to
/// semantic names (e.g. `MenuTab`, `EditPaste`, `BackspaceWithCtrl`) once
/// callers are reverse-engineered well enough to identify intent.
///
/// `#[repr(u32)]` with explicit discriminants so `transmute` round-trips
/// cleanly between the WA-side `u32` and this enum (see `TryFrom<u32>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum KeyboardAction {
    /// 0x01 — VK_TAB (simple)
    A01 = 0x01,
    /// 0x02 — VK_SPACE (simple)
    A02 = 0x02,
    /// 0x03 — VK_RETURN (simple)
    A03 = 0x03,
    /// 0x04 — VK_RETURN (simple, alias of 0x03)
    A04 = 0x04,
    /// 0x05 — VK_BACK (simple)
    A05 = 0x05,
    /// 0x06 — VK_LEFT (simple)
    A06 = 0x06,
    /// 0x07 — VK_RIGHT (simple)
    A07 = 0x07,
    /// 0x08 — VK_UP (simple)
    A08 = 0x08,
    /// 0x09 — VK_DOWN (simple)
    A09 = 0x09,
    /// 0x0A — bespoke: `UP || DOWN || junk(0xFF) || CLEAR`. The third probe's
    /// `OR AL, 0xFF` is a WA source bug that always queries undefined VK
    /// 0xFF; replicated byte-for-byte.
    A0A = 0x0A,
    /// 0x0B — VK_SHIFT (simple)
    A0B = 0x0B,
    /// 0x0C — VK_T (simple)
    A0C = 0x0C,
    /// 0x0D — bespoke: HOME with CTRL re-fire latch. With CTRL held, HOME
    /// fires every frame via `prev_state[VK_HOME] = 2` rearming.
    A0D = 0x0D,
    /// 0x0E — VK_SPACE (simple, alias of 0x02)
    A0E = 0x0E,
    /// 0x0F — VK_HOME (simple)
    A0F = 0x0F,
    /// 0x10 — VK_LEFT, gated on `key_state[VK_CONTROL] == 0`
    A10 = 0x10,
    /// 0x11 — VK_RIGHT, gated on `key_state[VK_CONTROL] == 0`
    A11 = 0x11,
    /// 0x12 — VK_DELETE, gated on `key_state[VK_CONTROL] == 0`
    A12 = 0x12,
    /// 0x13 — VK_BACK, gated on `key_state[VK_CONTROL] == 0`
    A13 = 0x13,
    /// 0x14 — VK_HOME (simple, alias of 0x0F)
    A14 = 0x14,
    /// 0x15 — VK_END (simple)
    A15 = 0x15,
    /// 0x16 — bespoke: paste-style "Shift+Insert OR Ctrl+V"
    A16 = 0x16,
    /// 0x17 — VK_LEFT, gated on `key_state[VK_CONTROL] != 0`
    A17 = 0x17,
    /// 0x18 — VK_RIGHT, gated on `key_state[VK_CONTROL] != 0`
    A18 = 0x18,
    /// 0x19 — VK_DELETE, gated on `key_state[VK_CONTROL] != 0`
    A19 = 0x19,
    /// 0x1A — VK_BACK, gated on `key_state[VK_CONTROL] != 0`
    A1A = 0x1A,
    /// 0x1B — default (return 0)
    A1B = 0x1B,
    /// 0x1C — default (return 0)
    A1C = 0x1C,
    /// 0x1D — default (return 0)
    A1D = 0x1D,
    /// 0x1E — default (return 0)
    A1E = 0x1E,
    /// 0x1F — VK_LEFT (simple, alias of 0x06)
    A1F = 0x1F,
    /// 0x20 — VK_RIGHT (simple, alias of 0x07)
    A20 = 0x20,
    /// 0x21 — VK_LEFT (simple, alias of 0x06)
    A21 = 0x21,
    /// 0x22 — VK_RIGHT (simple, alias of 0x07)
    A22 = 0x22,
    /// 0x23 — VK_ESCAPE (simple)
    Escape = 0x23,
    /// 0x24 — default (return 0)
    A24 = 0x24,
    /// 0x25 — VK_INSERT (simple)
    A25 = 0x25,
    /// 0x26 — VK_DELETE, gated on `!shift && !alt`
    A26 = 0x26,
    /// 0x27 — VK_DELETE, gated on `!shift && alt`
    A27 = 0x27,
    /// 0x28 — VK_DELETE, gated on `shift && !alt`
    A28 = 0x28,
    /// 0x29 — VK_DELETE, gated on `shift && alt`
    A29 = 0x29,
    /// 0x2A — default (return 0)
    A2A = 0x2A,
    /// 0x2B — default (return 0)
    A2B = 0x2B,
    /// 0x2C — default (return 0)
    A2C = 0x2C,
    /// 0x2D — VK_MENU/ALT (simple)
    A2D = 0x2D,
    /// 0x2E — VK_MENU/ALT (simple, alias of 0x2D)
    A2E = 0x2E,
    /// 0x2F — VK_R (simple)
    A2F = 0x2F,
    /// 0x30 — VK_T (simple, alias of 0x0C)
    A30 = 0x30,
    /// 0x31 — VK_S (simple)
    A31 = 0x31,
    /// 0x32 — VK_M (simple)
    A32 = 0x32,
    /// 0x33 — VK_SHIFT (simple, alias of 0x0B)
    A33 = 0x33,
    /// 0x34 — VK_MENU/ALT (simple, alias of 0x2D)
    A34 = 0x34,
    /// 0x35 — VK_SPACE (simple, alias of 0x02)
    A35 = 0x35,
    /// 0x36 — bespoke: Shift+Esc consume + clear OS keyboard state.
    Minimize = 0x36,
    /// 0x37 — VK '0' (computed `action - 7`)
    A37 = 0x37,
    /// 0x38 — VK '1' (computed)
    A38 = 0x38,
    /// 0x39 — VK '2' (computed)
    A39 = 0x39,
    /// 0x3A — VK '3' (computed)
    A3A = 0x3A,
    /// 0x3B — VK '4' (computed)
    A3B = 0x3B,
    /// 0x3C — VK '5' (computed)
    A3C = 0x3C,
    /// 0x3D — VK '6' (computed)
    A3D = 0x3D,
    /// 0x3E — VK '7' (computed)
    A3E = 0x3E,
    /// 0x3F — VK '8' (computed)
    A3F = 0x3F,
    /// 0x40 — VK '9' (computed)
    A40 = 0x40,
    /// 0x41 — VK '0' (simple)
    A41 = 0x41,
    /// 0x42 — bespoke: tilde/backtick layout-aware probe via
    /// `g_GameInfo._field_f384` flag bits.
    A42 = 0x42,
    /// 0x43 — VK_F1 (computed `action + 0x2D`)
    A43 = 0x43,
    /// 0x44 — VK_F2 (computed)
    A44 = 0x44,
    /// 0x45 — VK_F3 (computed)
    A45 = 0x45,
    /// 0x46 — VK_F4 (computed)
    A46 = 0x46,
    /// 0x47 — VK_F5 (computed)
    A47 = 0x47,
    /// 0x48 — VK_F6 (computed)
    A48 = 0x48,
    /// 0x49 — VK_F7 (computed)
    A49 = 0x49,
    /// 0x4A — VK_F8 (computed)
    A4A = 0x4A,
    /// 0x4B — VK_F9 (computed)
    A4B = 0x4B,
    /// 0x4C — VK_F10 (computed)
    A4C = 0x4C,
    /// 0x4D — VK_F11 (computed)
    A4D = 0x4D,
    /// 0x4E — VK_F12 (computed)
    A4E = 0x4E,
    /// 0x4F — bespoke: VK_OEM_MINUS OR VK_NUMPAD_SUBTRACT
    A4F = 0x4F,
    /// 0x50 — bespoke: VK_NUMPAD_ADD OR VK_OEM_PLUS
    A50 = 0x50,
    /// 0x51 — VK_F1 (computed `action + 0x1F`, alias of 0x43)
    A51 = 0x51,
    /// 0x52 — VK_F2 (computed, alias of 0x44)
    A52 = 0x52,
    /// 0x53 — VK_F3 (computed, alias of 0x45)
    A53 = 0x53,
    /// 0x54 — VK_F4 (computed, alias of 0x46)
    A54 = 0x54,
    /// 0x55 — VK_F5 (computed, alias of 0x47)
    A55 = 0x55,
    /// 0x56 — VK_F6 (computed, alias of 0x48)
    A56 = 0x56,
    /// 0x57 — VK_F7 (computed, alias of 0x49)
    A57 = 0x57,
    /// 0x58 — VK_F8 (computed, alias of 0x4A)
    A58 = 0x58,
    /// 0x59 — VK_F9 (computed, alias of 0x4B)
    A59 = 0x59,
    /// 0x5A — VK_F10 (computed, alias of 0x4C)
    A5A = 0x5A,
    /// 0x5B — VK_F11 (computed, alias of 0x4D)
    A5B = 0x5B,
    /// 0x5C — VK_F12 (computed, alias of 0x4E)
    A5C = 0x5C,
    /// 0x5D — VK_CONTROL (simple)
    A5D = 0x5D,
    /// 0x5E — VK_NEXT/PageDown (simple)
    A5E = 0x5E,
    /// 0x5F — VK_PRIOR/PageUp (simple)
    A5F = 0x5F,
    /// 0x60 — VK_NEXT/PageDown, gated on `key_state[VK_CONTROL] != 0`
    A60 = 0x60,
    /// 0x61 — VK_PRIOR/PageUp, gated on `key_state[VK_CONTROL] != 0`
    A61 = 0x61,
    /// 0x62 — VK_DOWN, gated on `key_state[VK_CONTROL] != 0`
    A62 = 0x62,
    /// 0x63 — VK_UP, gated on `key_state[VK_CONTROL] != 0`
    A63 = 0x63,
    /// 0x64 — VK '0' (computed `action - 0x34`, alias of 0x37)
    A64 = 0x64,
    /// 0x65 — VK '1' (computed, alias of 0x38)
    A65 = 0x65,
    /// 0x66 — VK '2' (computed, alias of 0x39)
    A66 = 0x66,
    /// 0x67 — VK '3' (computed, alias of 0x3A)
    A67 = 0x67,
    /// 0x68 — VK '4' (computed, alias of 0x3B)
    A68 = 0x68,
    /// 0x69 — VK '5' (computed, alias of 0x3C)
    A69 = 0x69,
    /// 0x6A — VK '6' (computed, alias of 0x3D)
    A6A = 0x6A,
    /// 0x6B — VK '7' (computed, alias of 0x3E)
    A6B = 0x6B,
    /// 0x6C — VK '8' (computed, alias of 0x3F)
    A6C = 0x6C,
    /// 0x6D — VK '9' (computed, alias of 0x40)
    A6D = 0x6D,
    /// 0x6E — bespoke: `GetKeyState(VK_SCROLL) & 1` (toggle bit, ignores mode)
    A6E = 0x6E,
    /// 0x6F — VK_SHIFT (simple, alias of 0x0B)
    A6F = 0x6F,
}

impl TryFrom<u32> for KeyboardAction {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if (0x01..=0x6F).contains(&value) {
            // SAFETY: every value in 0x01..=0x6F has a corresponding variant.
            Ok(unsafe { core::mem::transmute::<u32, KeyboardAction>(value) })
        } else {
            Err(value)
        }
    }
}

impl KeyboardAction {
    /// Edge-trigger query — returns true on the first probe of a frame while
    /// the underlying key is down; doesn't consume the latch (subsequent
    /// `is_pressed` probes within the same frame still see the press).
    /// Vtable slot 1 (`Keyboard::IsActionPressed`).
    #[inline]
    pub unsafe fn is_pressed(self, keyboard: *mut Keyboard) -> bool {
        unsafe { ((*(*keyboard).vtable).is_action_pressed)(keyboard, self as u32) != 0 }
    }

    /// Edge-trigger consuming query — returns true on the first probe of a
    /// frame while the underlying key is down, then arms the latch so further
    /// `is_active` probes return false until the key is released and re-pressed.
    /// Vtable slot 2 (`Keyboard::IsActionActive`).
    #[inline]
    pub unsafe fn is_active(self, keyboard: *mut Keyboard) -> bool {
        unsafe { ((*(*keyboard).vtable).is_action_active)(keyboard, self as u32) != 0 }
    }

    /// Edge-trigger consuming query, distinct latch slot from `is_active`.
    /// Some action codes (notably 0x0D and 0x36) treat the stored latch
    /// value (1 vs 2) as state. Vtable slot 3 (`Keyboard::IsActionActive2`).
    #[inline]
    pub unsafe fn is_active2(self, keyboard: *mut Keyboard) -> bool {
        unsafe { ((*(*keyboard).vtable).is_action_active2)(keyboard, self as u32) != 0 }
    }

    /// Level-triggered query — returns true while the action's latch is set,
    /// without modifying state. Vtable slot 4 (`Keyboard::IsActionHeld`).
    #[inline]
    pub unsafe fn is_held(self, keyboard: *mut Keyboard) -> bool {
        unsafe { ((*(*keyboard).vtable).is_action_held)(keyboard, self as u32) != 0 }
    }
}

/// Port of `Keyboard__CheckAction` (0x00571BA0).
///
/// `__usercall(EAX=action, ESI=this, EDI=mode) -> u32`. Once all four
/// IsAction* vtable wrappers (slots 1/2/3/4) are replaced in Rust, this is
/// the *only* path that resolves an action code to a key probe — the original
/// 0x00571BA0 is no longer reached WA-side. The action map is hand-baked from
/// WA's compact-switch tables at 0x00572198 / 0x005720CC; see
/// `project_keyboard_check_action.md` in memory for the full RE.
pub unsafe fn keyboard_check_action(this: *mut Keyboard, action: KeyboardAction, mode: i32) -> u32 {
    use KeyboardAction::*;
    unsafe {
        // Modifier shadows — WA reads them at +0x12C/+0x12D/+0x12E, which are
        // `key_state[VK_SHIFT/VK_CONTROL/VK_MENU]`.
        let shift = (*this).key_state[VK_SHIFT_VK as usize] != 0;
        let ctrl = (*this).key_state[VK_CONTROL_VK as usize] != 0;
        let alt = (*this).key_state[VK_MENU_VK as usize] != 0;

        let simple = |vk: u8| check_key_state(this, vk, mode) as u32;
        let guard_ctrl_eq0 = |vk: u8| if ctrl { 0 } else { simple(vk) };
        let guard_ctrl_ne0 = |vk: u8| if !ctrl { 0 } else { simple(vk) };

        match action {
            // ── Simple single-key probes ────────────────────────────────────
            A01 => simple(VK_TAB),
            A02 | A0E | A35 => simple(VK_SPACE),
            A03 | A04 => simple(VK_RETURN),
            A05 => simple(VK_BACK),
            A06 | A1F | A21 => simple(VK_LEFT),
            A07 | A20 | A22 => simple(VK_RIGHT),
            A08 => simple(VK_UP),
            A09 => simple(VK_DOWN),
            A0B | A33 | A6F => simple(VK_SHIFT_VK),
            A0C | A30 => simple(VK_T),
            A0F | A14 => simple(VK_HOME),
            A15 => simple(VK_END),
            Escape => simple(VK_ESCAPE),
            A25 => simple(VK_INSERT),
            A2D | A2E | A34 => simple(VK_MENU_VK),
            A2F => simple(VK_R),
            A31 => simple(VK_S),
            A32 => simple(VK_M),
            A41 => simple(0x30), // '0' (top row)
            A5D => simple(VK_CONTROL_VK),
            A5E => simple(VK_NEXT),
            A5F => simple(VK_PRIOR),

            // ── Computed VK ─────────────────────────────────────────────────
            A37 | A38 | A39 | A3A | A3B | A3C | A3D | A3E | A3F | A40 => {
                simple((action as u32 - 7) as u8) // '0'..'9'
            }
            A43 | A44 | A45 | A46 | A47 | A48 | A49 | A4A | A4B | A4C | A4D | A4E => {
                simple((action as u32 + 0x2D) as u8) // F1..F12
            }
            A51 | A52 | A53 | A54 | A55 | A56 | A57 | A58 | A59 | A5A | A5B | A5C => {
                simple((action as u32 + 0x1F) as u8) // F1..F12 alias
            }
            A64 | A65 | A66 | A67 | A68 | A69 | A6A | A6B | A6C | A6D => {
                simple((action as u32 - 0x34) as u8) // '0'..'9' alias
            }

            // ── Ctrl-modal navigation ───────────────────────────────────────
            A10 => guard_ctrl_eq0(VK_LEFT),
            A11 => guard_ctrl_eq0(VK_RIGHT),
            A12 => guard_ctrl_eq0(VK_DELETE),
            A13 => guard_ctrl_eq0(VK_BACK),
            A17 => guard_ctrl_ne0(VK_LEFT),
            A18 => guard_ctrl_ne0(VK_RIGHT),
            A19 => guard_ctrl_ne0(VK_DELETE),
            A1A => guard_ctrl_ne0(VK_BACK),
            A60 => guard_ctrl_ne0(VK_NEXT),
            A61 => guard_ctrl_ne0(VK_PRIOR),
            A62 => guard_ctrl_ne0(VK_DOWN),
            A63 => guard_ctrl_ne0(VK_UP),

            // ── Bespoke #1: arrow / clear OR-chain ──────────────────────────
            //
            // WA's body has a `MOV AL,0x26 / MOV AL,0x28 / OR AL,0xFF / MOV AL,0x0C`
            // sequence. The third probe's `OR AL,0xFF` unconditionally sets AL
            // to 0xFF (since AL retains the prior probe's keycode), so it queries
            // the undefined VK 0xFF — almost certainly a WA source bug. We
            // replicate it byte-for-byte: the probe always reads zero so it has
            // no behavioral effect, but the call still consumes the read.
            A0A => {
                if check_key_state(this, VK_UP, mode) {
                    return 1;
                }
                if check_key_state(this, VK_DOWN, mode) {
                    return 1;
                }
                if check_key_state(this, 0xFF, mode) {
                    return 1;
                }
                check_key_state(this, VK_CLEAR, mode) as u32
            }

            // ── Bespoke #2: HOME with CTRL re-fire latch ────────────────────
            A0D => {
                if !check_key_state(this, VK_HOME, mode) {
                    (*this).home_ctrl_latch = 0;
                    return (*this).home_ctrl_latch;
                }
                if !ctrl {
                    (*this).home_ctrl_latch = 0;
                    return 1;
                }
                // CTRL held: re-arm so HOME edges fire next frame too.
                (*this).prev_state[VK_HOME as usize] = 2;
                (*this).home_ctrl_latch = 1;
                (*this).home_ctrl_latch
            }

            // ── Bespoke #3: paste-style "Shift+Insert OR Ctrl+V" ────────────
            A16 => {
                if shift && !ctrl && !alt && check_key_state(this, VK_INSERT, mode) {
                    return 1;
                }
                if ctrl && !shift && !alt {
                    return check_key_state(this, VK_V, mode) as u32;
                }
                0
            }

            // ── Bespoke #4..7: 4-way Shift×Alt cross of DELETE ──────────────
            A26 => {
                if shift || alt {
                    return 0;
                }
                simple(VK_DELETE)
            }
            A27 => {
                if shift || !alt {
                    return 0;
                }
                simple(VK_DELETE)
            }
            A28 => {
                if !shift || alt {
                    return 0;
                }
                simple(VK_DELETE)
            }
            A29 => {
                if !shift || !alt {
                    return 0;
                }
                simple(VK_DELETE)
            }

            // ── Bespoke #8: Shift+Esc consume + clear OS state ──────────────
            //
            // Used to swallow Shift+Esc so subsequent OS-level focus tracking
            // doesn't see the modifier as still held.
            Minimize => {
                if !shift || !check_key_state(this, VK_ESCAPE, mode) {
                    return 0;
                }
                (*this).key_state[VK_SHIFT_VK as usize] = 0;
                (*this).key_state[VK_ESCAPE as usize] = 0;
                (*this).prev_state[VK_SHIFT_VK as usize] = 0;
                (*this).prev_state[VK_ESCAPE as usize] = 0;

                use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                    GetKeyboardState, SetKeyboardState,
                };
                let mut buf: [u8; 256] = [0; 256];
                GetKeyboardState(buf.as_mut_ptr());
                buf[VK_SHIFT_VK as usize] = 0;
                buf[VK_ESCAPE as usize] = 0;
                SetKeyboardState(buf.as_ptr());
                1
            }

            // ── Bespoke #9: tilde / backtick layout-aware probe ─────────────
            //
            // Reads `g_GameInfo._field_f384` as flag bits. WA's compare is
            // obfuscated — `(flags - 1) & 1 != 0` ⇔ "bit 0 is CLEAR", and
            // `(flags + 1) & 2 != 0` ⇔ "bits 0 and 1 differ". We replicate the
            // arithmetic literally rather than collapse the polarity.
            A42 => {
                let game_info = *(rb(va::G_GAME_INFO) as *const *const GameInfo);
                if game_info.is_null() {
                    return 0;
                }
                let flags = (*game_info)._field_f384;
                let mut result: u32 = 0;

                use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                    MAPVK_VSC_TO_VK, MapVirtualKeyA, VkKeyScanA,
                };

                if flags.wrapping_sub(1) & 1 != 0 {
                    let vk = MapVirtualKeyA(0x29, MAPVK_VSC_TO_VK) as u8;
                    result = check_key_state(this, vk, mode) as u32;
                }
                if flags.wrapping_add(1) & 2 != 0 {
                    let vk = VkKeyScanA(b'`' as i8) as u8;
                    result |= check_key_state(this, vk, mode) as u32;
                }
                result
            }

            // ── OEM/numpad OR-pairs ─────────────────────────────────────────
            A4F => {
                if check_key_state(this, VK_OEM_MINUS, mode) {
                    return 1;
                }
                check_key_state(this, VK_NUMPAD_SUBTRACT, mode) as u32
            }
            A50 => {
                if check_key_state(this, VK_NUMPAD_ADD, mode) {
                    return 1;
                }
                check_key_state(this, VK_OEM_PLUS, mode) as u32
            }

            // ── Bespoke #10: Scroll Lock toggle ─────────────────────────────
            //
            // Reads the OS toggle bit, ignoring `mode` and `Keyboard` state.
            A6E => {
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
                (GetKeyState(VK_SCROLL as i32) as u32) & 1
            }

            // ── Defaults (return 0) ─────────────────────────────────────────
            A1B | A1C | A1D | A1E | A24 | A2A | A2B | A2C => 0,
        }
    }
}

// ─── IsAction* vtable shims ──────────────────────────────────────────────────
//
// All four are `__thiscall(this=ECX, action=stack)`, `RET 0x4`. They convert
// the WA-side `u32` to `KeyboardAction` and dispatch with a hardcoded mode.
// Codes outside 0x01..=0x6F return 0 (matches WA's `JA default` fallthrough).

#[inline]
unsafe fn dispatch_action(this: *mut Keyboard, action: u32, mode: i32) -> i32 {
    match KeyboardAction::try_from(action) {
        Ok(a) => unsafe { keyboard_check_action(this, a, mode) as i32 },
        Err(_) => 0,
    }
}

/// Port of `Keyboard__IsActionPressed` (0x00572210) — vtable slot 1.
pub unsafe extern "thiscall" fn keyboard_is_action_pressed(
    this: *mut Keyboard,
    action: u32,
) -> i32 {
    unsafe { dispatch_action(this, action, 0) }
}

/// Port of `Keyboard__IsActionActive` (0x00572230) — vtable slot 2.
pub unsafe extern "thiscall" fn keyboard_is_action_active(this: *mut Keyboard, action: u32) -> i32 {
    unsafe { dispatch_action(this, action, 1) }
}

/// Port of `Keyboard__IsActionActive2` (0x00572250) — vtable slot 3.
pub unsafe extern "thiscall" fn keyboard_is_action_active2(
    this: *mut Keyboard,
    action: u32,
) -> i32 {
    unsafe { dispatch_action(this, action, 2) }
}

/// Port of `Keyboard__IsActionHeld` (0x00572270) — vtable slot 4.
pub unsafe extern "thiscall" fn keyboard_is_action_held(this: *mut Keyboard, action: u32) -> i32 {
    unsafe { dispatch_action(this, action, -1) }
}
