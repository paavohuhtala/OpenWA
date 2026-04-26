//! Mouse input — small standalone helpers.
//!
//! There is no Mouse class in WA.exe. Mouse state lives directly on
//! `GameSession` (see `cursor_initial`, `cursor_x/_y`, `mouse_delta_x/_y`,
//! `mouse_button_state`, `mouse_acquired`, `cursor_recenter_request`), and
//! all mouse-message handling happens inline in `GameSession::WindowProc`
//! (`0x00572660`, not yet ported). The two free functions ported here plus
//! `cursor_clip_and_recenter` are the only standalone helpers worth a
//! dedicated module.
//!
//! Headful path only — replay tests cannot exercise any of these.

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
};
