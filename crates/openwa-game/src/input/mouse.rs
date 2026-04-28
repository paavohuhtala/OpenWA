//! Mouse input — cursor management helpers + the [`MouseInput`] adapter.
//!
//! Most mouse state lives directly on `GameSession` (see `cursor_initial`,
//! `cursor_x/_y`, `mouse_delta_x/_y`, `mouse_button_state`, `mouse_acquired`,
//! `cursor_recenter_request`); all mouse-message handling happens inline in
//! `GameSession::WindowProc` (`0x00572660`, not yet ported). The
//! cursor-management helpers below (`mouse_poll_and_acquire`,
//! `mouse_release_and_center`, `cursor_clip_and_recenter`) are headful-only
//! — replay tests cannot exercise them.
//!
//! [`MouseInput`] is a separate small adapter object with its own vtable
//! (0x0066A2E4). Allocated by `GameEngine__InitHardware` and stored at
//! `GameSession+0xB0` / `GameWorld+0x10`. Despite the historical "Palette"
//! name on its vtable, it has nothing to do with graphics — it forwards
//! the raw `g_GameSession.mouse_*` fields to consumers (e.g. the ESC
//! menu's `EscMenu_TickState1`) with per-button debounce.

use core::mem::transmute;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoA, HMONITOR, IntersectRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MapWindowPoints, MonitorFromRect,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ClipCursor, GetClientRect, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
    SetCursorPos, SystemParametersInfoA,
};

use crate::address::va;
use crate::engine::game_session::{GameSession, get_game_session};
use crate::input::keyboard::keyboard_poll_state;
use crate::rebase::rb;

/// Port of `Cursor__ClipAndRecenter` (0x00573180), `__cdecl void()`.
///
/// In fullscreen mode (`g_FullscreenFlag != 0`):
///  1. Get the frontend window's client rect, mapped to screen coordinates.
///  2. Look up the monitor under that rect via `MonitorFromRect` +
///     `GetMonitorInfoA`. The rect chosen is `monitor.work_area` normally, or
///     `monitor.monitor_rect` when `g_PostGameRestoreFlag != 0` (full-screen
///     restoration). If `MonitorFromRect` fails (no monitor found), fall back
///     to `SystemParametersInfoA(SPI_GETWORKAREA)` or `GetSystemMetrics(SM_CX/CY)`
///     under the same flag gate.
///  3. Intersect the chosen rect with the client rect, store its center in
///     `GameSession.screen_center_x/_y`, and `ClipCursor` to it.
/// Then unconditionally `SetCursorPos(screen_center_x, screen_center_y)`.
///
/// WA does the `MonitorFromRect`/`GetMonitorInfoA` lookups via `GetProcAddress`
/// on `g_User32Module` (Win9x compat). On our `i686-pc-windows-msvc` target
/// both APIs are statically linkable, so we call them directly via
/// `windows-sys` — behaviorally equivalent on Win10+.
pub unsafe extern "cdecl" fn cursor_clip_and_recenter() {
    unsafe {
        let session = get_game_session();
        if session.is_null() {
            return;
        }

        if *(rb(va::G_FULLSCREEN_FLAG) as *const u32) != 0 {
            let frontend_hwnd = *(rb(va::G_FRONTEND_HWND) as *const HWND);

            // Client rect in screen coords.
            let mut client_rect: RECT = core::mem::zeroed();
            GetClientRect(frontend_hwnd, &mut client_rect);
            // MapWindowPoints with hWndTo=NULL maps to screen; cPoints=2 treats
            // the RECT as two POINTs (top-left and bottom-right).
            MapWindowPoints(
                frontend_hwnd,
                core::ptr::null_mut(),
                &mut client_rect as *mut RECT as *mut POINT,
                2,
            );

            let post_game_restore = *(rb(va::G_POST_GAME_RESTORE_FLAG_MAYBE) as *const u32) != 0;

            // Pick the screen rect to clip into.
            let monitor: HMONITOR = MonitorFromRect(&client_rect, MONITOR_DEFAULTTONEAREST);
            let screen_rect: RECT = if !monitor.is_null() {
                let mut mi: MONITORINFO = core::mem::zeroed();
                mi.cbSize = core::mem::size_of::<MONITORINFO>() as u32;
                GetMonitorInfoA(monitor, &mut mi);
                if post_game_restore {
                    mi.rcMonitor
                } else {
                    mi.rcWork
                }
            } else if post_game_restore {
                RECT {
                    left: 0,
                    top: 0,
                    right: GetSystemMetrics(SM_CXSCREEN),
                    bottom: GetSystemMetrics(SM_CYSCREEN),
                }
            } else {
                let mut work: RECT = core::mem::zeroed();
                SystemParametersInfoA(
                    SPI_GETWORKAREA,
                    0,
                    &mut work as *mut RECT as *mut core::ffi::c_void,
                    0,
                );
                work
            };

            // Intersect & take center.
            let mut clipped: RECT = core::mem::zeroed();
            IntersectRect(&mut clipped, &client_rect, &screen_rect);
            (*session).screen_center_x = (clipped.left + clipped.right) / 2;
            (*session).screen_center_y = (clipped.top + clipped.bottom) / 2;
            ClipCursor(&clipped);
        }

        // Always recenter (read fresh — the fullscreen branch may have written).
        let session = get_game_session();
        SetCursorPos((*session).screen_center_x, (*session).screen_center_y);
    }
}

/// Port of `Mouse__PollAndAcquire` (0x00572620), `__cdecl void()`.
///
/// Re-grabs mouse + keyboard input after focus regain or a fresh game start:
///  1. `GetCursorPos(&session.cursor_initial)` — snapshot for later restore.
///  2. `cursor_clip_and_recenter()` — clip & recenter to the game window.
///  3. `session.mouse_acquired = 1`.
///  4. `Keyboard::PollState(session.keyboard)` — refresh key_state/prev_state.
///  5. `FrontendDialog::UpdateCursor(g_InGameFrontendDialog)` — re-apply
///     cursor visibility (still bridged).
///
/// Callers (all WA-side):
///  - `Unknown__OnSYSCOMMAND` — SC_RESTORE / SC_MAXIMIZE branch
///  - `Unknown__OnACTIVATE` — WA_ACTIVE/WA_CLICKACTIVE in fullscreen
///  - `FUN_004ED701` — focus-restore inner helper
///  - `GameSession::WindowProc` — fallback when WM_*BUTTONDOWN arrives while
///    `mouse_acquired == 0` (first click after alt-tab).
pub unsafe extern "cdecl" fn mouse_poll_and_acquire() {
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let session = get_game_session();
        if session.is_null() {
            return;
        }

        GetCursorPos(&raw mut (*session).cursor_initial);
        cursor_clip_and_recenter();

        let keyboard = (*session).keyboard;
        (*session).mouse_acquired = 1;
        if !keyboard.is_null() {
            keyboard_poll_state(keyboard);
        }

        // Bridge: FrontendDialog::UpdateCursor(g_InGameFrontendDialog) — stdcall.
        let update_cursor: unsafe extern "stdcall" fn(u32) =
            transmute(rb(va::FRONTEND_DIALOG_UPDATE_CURSOR) as usize);
        update_cursor(rb(va::G_INGAME_FRONTEND_DIALOG));
    }
}

/// Port of `Mouse__ReleaseAndCenter` (0x005725B0), `__cdecl void()`.
///
/// Releases the cursor grab and clears all input state — the Alt+G "ungrab
/// cursor" hotkey handler:
///  1. `ClipCursor(NULL)` — release the cursor clip.
///  2. `session.mouse_acquired = 0; session.mouse_button_state = 0;`
///  3. `keyboard.clear_key_states()` — zero both key_state and prev_state.
///  4. `SetCursorPos(cursor_initial.x, cursor_initial.y)` — restore the
///     cursor to where it was when the game session started.
///  5. `FrontendDialog::UpdateCursor(g_InGameFrontendDialog)` — re-apply
///     cursor visibility (still bridged).
///
/// Caller: `GameSession::WindowProc` only (Alt+G key combo with !Ctrl/!Shift
/// in fullscreen, action 0x47 case).
pub unsafe extern "cdecl" fn mouse_release_and_center() {
    unsafe {
        let session = get_game_session();
        if session.is_null() {
            return;
        }

        ClipCursor(core::ptr::null());

        let keyboard = (*session).keyboard;
        (*session).mouse_acquired = 0;
        (*session).mouse_button_state = 0;
        // WA dereferences keyboard unconditionally; mirror that exactly.
        (*keyboard).clear_key_states();

        SetCursorPos((*session).cursor_initial.x, (*session).cursor_initial.y);

        // Bridge: FrontendDialog::UpdateCursor(g_InGameFrontendDialog) — stdcall.
        let update_cursor: unsafe extern "stdcall" fn(u32) =
            transmute(rb(va::FRONTEND_DIALOG_UPDATE_CURSOR) as usize);
        update_cursor(rb(va::G_INGAME_FRONTEND_DIALOG));
    }
}

// Compile-only assertion: the GameSession field offsets WA's mouse helpers
// touch must match what we declared.
const _: () = {
    assert!(core::mem::offset_of!(GameSession, mouse_acquired) == 0x2C);
    assert!(core::mem::offset_of!(GameSession, mouse_button_state) == 0x88);
    assert!(core::mem::offset_of!(GameSession, cursor_initial) == 0x70);
    assert!(core::mem::offset_of!(GameSession, screen_center_x) == 0x54);
    assert!(core::mem::offset_of!(GameSession, screen_center_y) == 0x58);
    assert!(core::mem::offset_of!(GameSession, mouse_delta_x) == 0x78);
    assert!(core::mem::offset_of!(GameSession, mouse_delta_y) == 0x7C);
};

// ─── MouseInput adapter ────────────────────────────────────────────────────

/// Vtable for [`MouseInput`] (0x0066A2E4 — historically labelled
/// `Palette_vtable_Maybe` in Ghidra).
///
/// Slot 0 is a base-class destructor; slots 1–3 are the mouse-input API;
/// slot 4 is the shared `WorldEntity__vt19` no-op stub (kept for layout
/// fidelity with the original 5-slot vtable).
#[openwa_game::vtable(size = 5, va = 0x0066A2E4, class = "MouseInput")]
pub struct MouseInputVtable {
    /// Slot 0 — scalar deleting destructor (0x0056D2C0). Writes the base
    /// `&PTR_FUN_0066A2F8` vtable into `*this`, then `_free` if `flags & 1`.
    #[slot(0)]
    pub destructor: fn(this: *mut MouseInput, flags: u32) -> *mut MouseInput,
    /// Slot 1 — `Mouse__ConsumeDeltaAndButtons` (0x0056D2E0). Outputs the
    /// raw cursor deltas + a debounced button bitmask:
    ///
    /// ```text
    /// *out_dx = g_GameSession.mouse_delta_x       (+0x78)
    /// *out_dy = g_GameSession.mouse_delta_y       (+0x7C)
    /// let buttons = g_GameSession.mouse_button_state  (+0x88)
    /// *out_buttons = self.button_armed_latch & buttons
    /// self.button_armed_latch = self.button_armed_latch | !buttons
    /// ```
    ///
    /// A held button only registers once until the next release re-arms its
    /// bit. Does NOT clear the deltas — pair with [`Self::clear_deltas`].
    #[slot(1)]
    pub consume_delta_and_buttons:
        fn(this: *mut MouseInput, out_dx: *mut i32, out_dy: *mut i32, out_buttons: *mut u32),
    /// Slot 2 — `Mouse__AckButtonMask` (0x0056D320). AND-only ack helper:
    ///
    /// ```text
    /// self.button_armed_latch &= ~(g_GameSession.mouse_button_state & mask)
    /// ```
    ///
    /// Clears bits in the latch corresponding to currently-held buttons.
    /// Used to consume a pending click without polling deltas.
    #[slot(2)]
    pub ack_button_mask: fn(this: *mut MouseInput, mask: u32),
    /// Slot 3 — `Mouse__ClearDeltas` (0x0056D340). Zeroes
    /// `g_GameSession.mouse_delta_x` / `mouse_delta_y` (does NOT touch the
    /// latch). Standalone reset path.
    #[slot(3)]
    pub clear_deltas: fn(this: *mut MouseInput),
    /// Slot 4 — `WorldEntity__vt19` (0x004AA060). Generic base-class
    /// no-op stub shared with several other vtables; kept for layout
    /// fidelity. Called by `GameEngine__InitHardware` immediately after
    /// allocation but has no observable effect.
    #[slot(4)]
    pub slot_04_noop: fn(this: *mut MouseInput),
}

/// `MouseInput` — 0x28-byte mouse-input adapter object.
///
/// Allocated by `GameEngine__InitHardware` (0x0056D350) as a 0x28-byte
/// heap object initialised with `button_armed_latch = -1` (= all bits
/// armed, so the very first press of any button registers). Stored at
/// `GameSession+0xB0` and forwarded to `GameWorld+0x10`.
///
/// Most of the body is unused in WA — only the vtable pointer and the
/// [`Self::button_armed_latch`] field are accessed by the four `Mouse__*`
/// methods.
#[repr(C)]
pub struct MouseInput {
    /// 0x000: Vtable pointer (0x0066A2E4).
    pub vtable: *const MouseInputVtable,
    /// 0x004: Per-button "armed" latch. A bit set means "the next press of
    /// this button counts"; cleared when [`MouseInputVtable::consume_delta_and_buttons`]
    /// or [`MouseInputVtable::ack_button_mask`] consumes a press; re-armed
    /// when the button is released. Initialised to `0xFFFFFFFF` (all bits
    /// armed) by `GameEngine__InitHardware`.
    pub button_armed_latch: u32,
    /// 0x008-0x027: Trailing storage. Allocated 0x28 bytes but the four
    /// known methods never read past `+0x4` — kept opaque for fidelity.
    pub _unknown_008: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<MouseInput>() == 0x28);

bind_MouseInputVtable!(MouseInput, vtable);

impl MouseInput {
    /// Inline construction matching `GameEngine__InitHardware`'s setup:
    /// vtable + `button_armed_latch = -1` + zero-init trailing bytes.
    ///
    /// # Safety
    /// `vtable_addr` must be a valid rebased vtable pointer.
    pub unsafe fn new(vtable_addr: u32) -> Self {
        Self {
            vtable: vtable_addr as *const MouseInputVtable,
            button_armed_latch: 0xFFFFFFFF,
            _unknown_008: [0; 0x20],
        }
    }
}
