//! Rust port of `GameSession::PumpMessages` (0x00572E30).
//!
//! Pumps queued Win32 messages between frames. Used both directly by
//! `run_game_session` and (via the full-replacement hook) by
//! `GameRuntime::LoadingProgressTick` in WA.

use windows_sys::Win32::Foundation::HWND;

use crate::address::va;
use crate::engine::game_session::get_game_session;
use crate::frontend::input_hooks::{InputHookMode, unhook_input_hooks};
use crate::rebase::rb;

/// Rust port of `GameSession::PumpMessages` (0x00572E30).
///
/// Suppresses keyboard (`WM_KEYFIRST..=WM_KEYLAST`), mouse
/// (`WM_MOUSEFIRST..=WM_MOUSELAST`) and `WM_PALETTECHANGED` messages
/// destined for non-frontend windows when `g_InputHookMode == 0` — these
/// would otherwise leak to background MFC dialogs. `WM_QUIT` always
/// dispatches and additionally sets `exit_flag = flag_40 = 1`.
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
        DispatchMessageA, MSG, PM_REMOVE, PeekMessageA, SetCursorPos, TranslateMessage,
        WM_KEYFIRST, WM_KEYLAST, WM_MOUSEFIRST, WM_MOUSELAST, WM_PALETTECHANGED, WM_QUIT,
    };

    unsafe {
        *(rb(va::G_IN_GAME_LOOP) as *mut u32) = 1;

        let frontend_hwnd = *(rb(va::G_FRONTEND_HWND) as *const HWND);

        let mut msg: MSG = core::mem::zeroed();
        while PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            unhook_input_hooks();

            let session = get_game_session();
            let dispatch = if msg.message == WM_QUIT {
                (*session).exit_flag = 1;
                (*session).flag_40 = 1;
                true
            } else if InputHookMode::get() != InputHookMode::Off {
                true
            } else {
                let is_input_or_palette = matches!(
                    msg.message,
                    WM_KEYFIRST..=WM_KEYLAST
                        | WM_MOUSEFIRST..=WM_MOUSELAST
                        | WM_PALETTECHANGED
                );
                !is_input_or_palette || msg.hwnd == frontend_hwnd
            };

            if dispatch {
                TranslateMessage(&msg);
                DispatchMessageA(&msg);

                let cfg = (*session).config_ptr;
                if !cfg.is_null()
                    && (*cfg).headless_mode == 0
                    && *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8) == 0
                    && (*session).mouse_acquired != 0
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

        unhook_input_hooks();
    }
}
