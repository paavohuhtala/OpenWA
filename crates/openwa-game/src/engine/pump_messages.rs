//! Rust port of `GameSession::PumpMessages` (0x00572E30).
//!
//! Pumps queued Win32 messages between frames. Used both directly by
//! `run_game_session` and (via the full-replacement hook) by
//! `GameRuntime::LoadingProgressTick` in WA.

use core::mem::transmute;

use windows_sys::Win32::Foundation::HWND;

use crate::address::va;
use crate::engine::game_session::get_game_session;
use crate::rebase::rb;

/// Rust port of `GameSession::PumpMessages` (0x00572E30).
///
/// Suppresses keyboard (0x100..=0x109), mouse (0x200..=0x20E) and 0x311
/// messages destined for non-frontend windows when `g_InputHookMode == 0`
/// — these would otherwise leak to background MFC dialogs. `WM_QUIT`
/// always dispatches and additionally sets `exit_flag = flag_40 = 1`.
///
/// After dispatching, recenters the cursor to `(screen_center_x,
/// screen_center_y)` once per iteration in normal play (gameplay clamps
/// the cursor to capture relative motion). Skipped in headless / display
/// mode and when `flag_2c == 0`.
///
/// `Frontend::UnhookInputHooks` is invoked at the top of each iteration
/// that has a message AND once after the loop exits — matching WA's
/// "two unhook sites" structure exactly. The unhook helper is a near-noop
/// in normal play (gated on `g_InputHookMode != 0`).
///
/// Two callers:
///  - `run_game_session` (Rust) — direct call.
///  - `GameRuntime::LoadingProgressTick` (still WA) — reaches us via the
///    full-replacement hook installed on the WA address.
pub unsafe extern "cdecl" fn pump_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageA, MSG, PM_REMOVE, PeekMessageA, SetCursorPos, TranslateMessage, WM_QUIT,
    };

    unsafe {
        *(rb(va::G_IN_GAME_LOOP) as *mut u32) = 1;

        let unhook: unsafe extern "cdecl" fn() = transmute(rb(va::FRONTEND_UNHOOK_INPUT_HOOKS));
        let frontend_hwnd = *(rb(va::G_FRONTEND_HWND) as *const HWND);

        let mut msg: MSG = core::mem::zeroed();
        while PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            unhook();

            let session = get_game_session();
            let dispatch = if msg.message == WM_QUIT {
                (*session).exit_flag = 1;
                (*session).flag_40 = 1;
                true
            } else if *(rb(va::G_INPUT_HOOK_MODE) as *const u32) != 0 {
                true
            } else {
                let m = msg.message;
                let is_keyboard = m.wrapping_sub(0x100) <= 9;
                let is_mouse = m.wrapping_sub(0x200) <= 0xE;
                let is_311 = m == 0x311;
                (!is_keyboard && !is_mouse && !is_311) || msg.hwnd == frontend_hwnd
            };

            if dispatch {
                TranslateMessage(&msg);
                DispatchMessageA(&msg);

                let cfg = (*session).config_ptr;
                if !cfg.is_null()
                    && (*cfg).headless_mode == 0
                    && *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8) == 0
                    && (*session).flag_2c != 0
                {
                    let cx = (*session).screen_center_x;
                    let cy = (*session).screen_center_y;
                    if (*session).cursor_x != cx || (*session).cursor_y != cy {
                        (*session).cursor_x = cx;
                        (*session).cursor_y = cy;
                        SetCursorPos(cx, cy);
                    }
                }
            }
        }

        unhook();
    }
}
