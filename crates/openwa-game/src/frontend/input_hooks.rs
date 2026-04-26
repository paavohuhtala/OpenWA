//! Frontend input-hook lifecycle helpers.
//!
//! WA installs a `WH_GETMESSAGE` keyboard hook and a `WH_FOREGROUNDIDLE`
//! mouse hook (`Frontend::InstallInputHooks`, 0x004ED3C0) when entering
//! a modal-dialog input mode. `Frontend::UnhookInputHooks` (0x004ED590)
//! tears them down again. The Rust port lives here.

use core::ptr;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Media::timeEndPeriod;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HHOOK, KillTimer, MSG, PM_NOYIELD, PM_REMOVE, PeekMessageA, SendMessageA, UnhookWindowsHookEx,
    WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MOUSEWHEEL,
};

use crate::address::va;
use crate::rebase::rb;

/// State of the frontend's modal-dialog input grab (`g_InputHookMode`).
///
/// Set by `Frontend::InstallInputHooks` (0x004ED3C0) and the
/// mode-transition helpers `Frontend::EnterInputMode1` (0x004ED420) /
/// `Frontend::EnterInputMode2` (0x004ED4F0); cleared by
/// `unhook_input_hooks`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputHookMode {
    /// No hooks installed; gameplay input flows normally.
    Off = 0,
    /// Blocking input grab — `WH_GETMESSAGE` keyboard hook +
    /// `WH_FOREGROUNDIDLE` mouse hook installed.
    Blocking = 1,
    /// `Blocking` plus a 10ms frontend-frame timer (`SetTimer` +
    /// `timeBeginPeriod(1)`); used while a frontend transition is animating.
    Animated = 2,
}

impl InputHookMode {
    /// Read the current value of `g_InputHookMode`. Panics on unknown
    /// values (only 0/1/2 are written by WA's transition helpers).
    #[inline]
    pub unsafe fn get() -> Self {
        unsafe {
            match *(rb(va::G_INPUT_HOOK_MODE) as *const u32) {
                0 => Self::Off,
                1 => Self::Blocking,
                2 => Self::Animated,
                n => panic!("unknown g_InputHookMode value: {n}"),
            }
        }
    }

    /// Reset `g_InputHookMode` to [`InputHookMode::Off`].
    #[inline]
    pub unsafe fn clear() {
        unsafe {
            *(rb(va::G_INPUT_HOOK_MODE) as *mut u32) = 0;
        }
    }
}

/// Magic timer ID set by `Frontend::EnterInputMode2` (0x004ED4F0) and
/// killed here when exiting mode 2. Bytes `4B 4C 42 4E` ("KLBN").
const FRONTEND_INPUT_TIMER_ID: usize = 0x4E424C4B;

/// Private app-defined message (`0xBFFB`) sent to the focused window after
/// `g_InputHookMode` is reset to 0 — notifies whoever is interested that
/// the input grab has been released.
const WM_FRONTEND_INPUT_RELEASED: u32 = 0xBFFB;

/// Rust port of `Frontend::UnhookInputHooks` (0x004ED590).
///
/// No-op if `g_InputHookMode == 0`. Otherwise:
///  1. `UnhookWindowsHookEx` on `g_KeyboardHook` / `g_MouseHook` (each if non-null).
///  2. If mode 2: `KillTimer(g_FrontendFrame->m_hWnd, FRONTEND_INPUT_TIMER_ID)`
///     and `timeEndPeriod(1)` to undo the matching `timeBeginPeriod`.
///  3. If mode 1, or `g_input_hook_flag_2dd7_Maybe != 0`: drain a stale
///     mouse event via `PeekMessageA` with a filter range selected by
///     `g_input_hook_filter_select_Maybe` (middle-button range vs wheel).
///  4. `g_InputHookMode = 0`; `SendMessageA(GetFocus(), 0xBFFB, 1, 0)` to
///     announce the input release.
///
/// Callers (besides the Rust `pump_messages`):
///  - `CDialog::CustomMsgPump` (two sites)
///  - `CDialog::DoModal_Custom`
///  - `FUN_0048DB10` (two sites)
///  - `FUN_004DF8F0` (two sites)
///
/// All WA-side callers reach this implementation via the full-replacement
/// hook installed at the WA address.
pub unsafe extern "cdecl" fn unhook_input_hooks() {
    unsafe {
        let mode = InputHookMode::get();
        if mode == InputHookMode::Off {
            return;
        }

        let kbd = *(rb(va::G_KEYBOARD_HOOK) as *const HHOOK);
        if !kbd.is_null() {
            UnhookWindowsHookEx(kbd);
        }
        let mouse = *(rb(va::G_MOUSE_HOOK) as *const HHOOK);
        if !mouse.is_null() {
            UnhookWindowsHookEx(mouse);
        }

        if mode == InputHookMode::Animated {
            let frame = *(rb(va::G_FRONTEND_FRAME) as *const *const u8);
            if !frame.is_null() {
                let hwnd = *(frame.add(0x20) as *const HWND);
                KillTimer(hwnd, FRONTEND_INPUT_TIMER_ID);
            }
            timeEndPeriod(1);
        }

        let flush_pending = *(rb(va::G_INPUT_HOOK_FLAG_2DD7_MAYBE) as *const u8);
        if mode == InputHookMode::Blocking || flush_pending != 0 {
            let filter_select = *(rb(va::G_INPUT_HOOK_FILTER_SELECT_MAYBE) as *const u8);
            let (min, max) = if filter_select != 0 {
                (WM_MBUTTONDOWN, WM_MBUTTONDBLCLK)
            } else {
                (WM_MOUSEWHEEL, WM_MOUSEWHEEL)
            };
            let mut msg: MSG = core::mem::zeroed();
            PeekMessageA(&mut msg, ptr::null_mut(), min, max, PM_REMOVE | PM_NOYIELD);
        }

        InputHookMode::clear();
        let focus = GetFocus();
        SendMessageA(focus, WM_FRONTEND_INPUT_RELEASED, 1, 0);
    }
}
