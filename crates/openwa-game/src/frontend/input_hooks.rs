//! Frontend input-hook lifecycle helpers.
//!
//! WA installs a `WH_GETMESSAGE` mouse-message filter hook plus a
//! `WH_FOREGROUNDIDLE` frame-ping hook on the calling thread
//! (`Frontend::InstallInputHooks`, 0x004ED3C0) when entering a modal-dialog
//! input mode. `Frontend::UnhookInputHooks` (0x004ED590) tears them down
//! again. Install/unhook lifecycle plus the WH_FOREGROUNDIDLE proc are
//! Rust. The WH_GETMESSAGE proc (`FUN_004ED160`) is still WA — GUI-only
//! mouse-message synthesis path, deferred until `GameSession::WindowProc`
//! is ported.

use core::ptr;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Media::timeEndPeriod;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HHOOK, HOOKPROC, KillTimer, MSG, PM_NOYIELD, PM_REMOVE, PeekMessageA,
    PostMessageA, SendMessageA, SetWindowsHookExA, UnhookWindowsHookEx, WH_FOREGROUNDIDLE,
    WH_GETMESSAGE, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MOUSEWHEEL,
};

use crate::address::va;
use crate::rebase::rb;

/// Address of the WH_GETMESSAGE hook proc inside WA.exe (`FUN_004ED160`).
/// Filters mouse messages — synthesizes `mouse_event` calls for
/// `WM_NCLBUTTONDOWN`/`WM_NCRBUTTONDOWN` and drains `WM_MOUSEWHEEL` /
/// `WM_MBUTTON*` queued bursts. Calls `GameSession::ProcessFrame` if a
/// session exists; deferred from porting (GUI-only path).
const FRONTEND_GET_MESSAGE_PROC_VA: u32 = 0x004ED160;

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

/// Private app-defined message (`0xBFFC`) posted to the frontend frame's
/// HWND each time the foreground thread idles in
/// [`InputHookMode::Animated`] — keeps the animated frontend redrawing
/// while no Win32 input is being processed.
const WM_FRONTEND_ANIMATE_TICK: u32 = 0xBFFC;

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

        let kbd = *(rb(va::G_FRONTEND_MSG_HOOK) as *const HHOOK);
        if !kbd.is_null() {
            UnhookWindowsHookEx(kbd);
        }
        let mouse = *(rb(va::G_FRONTEND_IDLE_HOOK) as *const HHOOK);
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

/// Rust port of `Frontend::InstallInputHooks` (0x004ED3C0).
///
/// Installs two thread-local Win32 hooks on the calling thread:
///  - `WH_GETMESSAGE` (proc at `FRONTEND_GET_MESSAGE_PROC_VA`) — stored in
///    `g_FrontendMsgHook`. Filters mouse messages.
///  - `WH_FOREGROUNDIDLE` (proc at `FRONTEND_FOREGROUND_IDLE_PROC_VA`) —
///    stored in `g_FrontendIdleHook`. Pings the frontend frame on idle.
///
/// Also picks the `g_input_hook_filter_select` byte from the OS-version
/// pair (`g_DesktopCheckLevel` + `g_OsVersionSub`) — `1` on modern NT 6+
/// (uses `WM_MBUTTON*` range), `0` on legacy NT4/9x (uses `WM_MOUSEWHEEL`).
///
/// Does NOT touch `g_InputHookMode`; the callers (mode-1 / mode-2 entry
/// helpers at 0x004ED420 / 0x004ED4F0) write the mode immediately before
/// invoking us.
///
/// Callers (both still WA-side):
///  - `enter_blocking_input_mode` (0x004ED420, mode 1 entry)
///  - `enter_animated_input_mode` (0x004ED4F0, mode 2 entry — tail JMP)
pub unsafe extern "cdecl" fn install_input_hooks() {
    unsafe {
        // OS-version gate: `WM_MBUTTON*` filter range on NT6+, `WM_MOUSEWHEEL`
        // otherwise. The compare in WA is `level <= 1 || (level == 2 && sub >= 10)`.
        let level = *(rb(va::G_DESKTOP_CHECK_LEVEL) as *const u32);
        let sub = *(rb(va::G_OS_VERSION_SUB) as *const u32);
        let filter_select: u8 = if level < 2 || (level == 2 && sub >= 10) {
            1
        } else {
            0
        };
        *(rb(va::G_INPUT_HOOK_FILTER_SELECT_MAYBE) as *mut u8) = filter_select;

        let msg_proc: HOOKPROC = Some(core::mem::transmute::<
            usize,
            unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT,
        >(rb(FRONTEND_GET_MESSAGE_PROC_VA) as usize));

        let tid = GetCurrentThreadId();
        let msg_hook = SetWindowsHookExA(WH_GETMESSAGE, msg_proc, 0 as HINSTANCE, tid);
        *(rb(va::G_FRONTEND_MSG_HOOK) as *mut HHOOK) = msg_hook;

        let tid = GetCurrentThreadId();
        let idle_hook = SetWindowsHookExA(
            WH_FOREGROUNDIDLE,
            Some(foreground_idle_proc),
            0 as HINSTANCE,
            tid,
        );
        *(rb(va::G_FRONTEND_IDLE_HOOK) as *mut HHOOK) = idle_hook;
    }
}

/// Rust port of `Frontend::ForegroundIdleProc` (0x004ED0D0).
///
/// Win32 `WH_FOREGROUNDIDLE` hook proc — fires on the calling thread when
/// the message-pump goes idle. While in [`InputHookMode::Animated`], posts
/// a `WM_FRONTEND_ANIMATE_TICK` ping to the frontend frame's HWND so its
/// transition animation keeps redrawing even when no input is arriving.
/// Otherwise just chains via `CallNextHookEx`.
///
/// `g_input_hook_flag_2dd7_Maybe != 0` suppresses the ping — the
/// WH_GETMESSAGE proc sets that flag while it's already pumping a
/// synthetic mouse event, so we don't double-pulse the animation.
///
/// Registered by [`install_input_hooks`]; the WA-side address
/// (`va::FRONTEND_FOREGROUND_IDLE_PROC`) is install-trapped as a safety
/// net since no other code references it.
pub unsafe extern "system" fn foreground_idle_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let frame = *(rb(va::G_FRONTEND_FRAME) as *const *const u8);
        let suppress = *(rb(va::G_INPUT_HOOK_FLAG_2DD7_MAYBE) as *const u8);
        if !frame.is_null() && InputHookMode::get() == InputHookMode::Animated && suppress == 0 {
            let hwnd = *(frame.add(0x20) as *const HWND);
            PostMessageA(hwnd, WM_FRONTEND_ANIMATE_TICK, 1, 0);
        }
        let idle_hook = *(rb(va::G_FRONTEND_IDLE_HOOK) as *const HHOOK);
        CallNextHookEx(idle_hook, code, wparam, lparam)
    }
}
