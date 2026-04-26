//! Frontend input-hook lifecycle helpers.
//!
//! WA installs a `WH_GETMESSAGE` mouse-message filter hook plus a
//! `WH_FOREGROUNDIDLE` frame-ping hook on the calling thread
//! (`Frontend::InstallInputHooks`, 0x004ED3C0) when entering a modal-dialog
//! input mode. `Frontend::UnhookInputHooks` (0x004ED590) tears them down
//! again. Install/unhook lifecycle and both hook procs are now Rust;
//! [`install_input_hooks`] registers the Rust functions directly with
//! `SetWindowsHookExA`, so the WA-side proc addresses are install-trapped
//! as safety nets.

use core::ptr;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Media::timeEndPeriod;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, GetFocus, GetKeyState, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_WHEEL, VK_MBUTTON, mouse_event,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageA, FindWindowA, GetQueueStatus, HHOOK, KillTimer, MSG,
    PM_NOYIELD, PM_REMOVE, PeekMessageA, PostMessageA, QS_MOUSE, SendMessageA, SetWindowsHookExA,
    UnhookWindowsHookEx, WH_FOREGROUNDIDLE, WH_GETMESSAGE, WHEEL_DELTA, WM_MBUTTONDBLCLK,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEWHEEL, WM_NCLBUTTONDOWN, WM_NCRBUTTONDOWN,
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
///  - `WH_GETMESSAGE` ([`get_message_proc`]) — stored in `g_FrontendMsgHook`.
///    Filters mouse messages.
///  - `WH_FOREGROUNDIDLE` ([`foreground_idle_proc`]) — stored in
///    `g_FrontendIdleHook`. Pings the frontend frame on idle.
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

        let tid = GetCurrentThreadId();
        let msg_hook =
            SetWindowsHookExA(WH_GETMESSAGE, Some(get_message_proc), 0 as HINSTANCE, tid);
        *(rb(va::G_FRONTEND_MSG_HOOK) as *mut HHOOK) = msg_hook;

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

// ─── WH_GETMESSAGE hook proc ──────────────────────────────────────────────

/// Recursion guard byte (`g_GetMsgProc_Recursing` at 0x008ACE32). Set on
/// entry to [`get_message_proc`] when it's about to do real work and cleared
/// on exit; treated as "skip processing, just chain" if observed nonzero.
const G_GETMSG_RECURSING: u32 = 0x008ACE32;

/// State-changed ack byte (`g_GetMsgProc_StateAck_Maybe` at 0x008ACE33).
/// Toggled by the off/blocking drain branch as a "we just updated the
/// pressed state, ack it on the next matching message" latch.
const G_GETMSG_STATE_ACK: u32 = 0x008ACE33;

/// Set on entry to [`get_message_proc`]'s NC-button branch:
/// `0` for `WM_NCLBUTTONDOWN`, `1` for `WM_NCRBUTTONDOWN`. Used later to
/// gate the off/blocking-pending drain path between the "no-drain"
/// (off-mode natural) branch and the "drain queued button-class messages
/// before synth" branch.
const G_GETMSG_NC_RIGHT: u32 = 0x006B39C1;

/// Cached middle-button "pressed" byte used to choose between
/// `MOUSEEVENTF_MIDDLEDOWN` (0x20, set) and `MOUSEEVENTF_MIDDLEUP` (0x40,
/// clear) for the synthesised `mouse_event`.
const G_GETMSG_MBTN_PRESSED: u32 = 0x006B32AB;

/// Top-of-modal-dialog-stack pointer (`g_TopModalDialog_Maybe` at
/// 0x007A03DC). Written by `CDialog::DoModal_Custom` / `OnDESTROY`; read
/// by [`pump_modal_or_session_frame`] to find the dialog whose WM_TIMER
/// queue should be drained.
const G_TOP_MODAL_DIALOG: u32 = 0x007A03DC;

/// Win32 popup-menu window class (`FindWindowA` is used to detect that a
/// popup menu is currently visible). Matches WA's literal `"#32768"`.
const POPUP_MENU_CLASS: &[u8] = b"#32768\0";

/// Vtable offset on a frontend `CDialog` for the per-tick transition
/// method. Same slot is invoked by `FrontendChangeScreen` (with arg 1
/// then 2) and by [`pump_modal_or_session_frame`] (with arg 0).
const DIALOG_VTABLE_TRANSITION_METHOD: usize = 0x15C;

/// Synthesised `mouse_event` wheel-delta value used by the off-mode wheel
/// path (`0x78` = `WHEEL_DELTA` = one detent).
const SYNTH_WHEEL_DELTA: i32 = WHEEL_DELTA as i32;

/// Inlined Rust port of `Frontend::PumpModalOrSessionFrame` (0x004ED050) —
/// the per-callback frame-work helper for [`get_message_proc`].
///
/// If a `GameSession` exists with `init_flag != 0`, runs a single
/// `GameSession::ProcessFrame` tick. Otherwise, if a top-level modal
/// dialog is active, drains its WM_TIMER (0x113) message queue with
/// `DispatchMessageA`, then invokes the dialog's vtable+0x15C transition
/// method with arg `0`. Re-checks `g_TopModalDialog` after each dispatch
/// since the dialog may destroy itself mid-loop.
unsafe fn pump_modal_or_session_frame() {
    unsafe {
        let session =
            *(rb(va::G_GAME_SESSION) as *const *mut crate::engine::game_session::GameSession);
        if !session.is_null() && (*session).init_flag != 0 {
            crate::engine::main_loop::process_frame::process_frame();
            return;
        }

        let dialog = *(rb(G_TOP_MODAL_DIALOG) as *const *mut u8);
        if dialog.is_null() {
            return;
        }
        loop {
            let hwnd = *(dialog.add(0x20) as *const HWND);
            let mut msg: MSG = core::mem::zeroed();
            if PeekMessageA(&mut msg, hwnd, 0x113, 0x113, PM_REMOVE) == 0 {
                break;
            }
            DispatchMessageA(&msg);
            if (*(rb(G_TOP_MODAL_DIALOG) as *const *mut u8)).is_null() {
                return;
            }
        }
        let dialog = *(rb(G_TOP_MODAL_DIALOG) as *const *mut u8);
        if dialog.is_null() {
            return;
        }
        let vtable = *(dialog as *const u32);
        let slot_addr = vtable as usize + DIALOG_VTABLE_TRANSITION_METHOD;
        let fn_ptr = *(slot_addr as *const u32);
        let f: unsafe extern "fastcall" fn(*mut u8, u32, u32) = core::mem::transmute(fn_ptr);
        f(dialog, 0, 0);
    }
}

#[inline]
unsafe fn synth_wheel_event() {
    unsafe { mouse_event(MOUSEEVENTF_WHEEL, 0, 0, SYNTH_WHEEL_DELTA, 0) }
}

#[inline]
unsafe fn synth_middle_button(pressed: bool) {
    let flags = if pressed {
        MOUSEEVENTF_MIDDLEDOWN
    } else {
        MOUSEEVENTF_MIDDLEUP
    };
    unsafe { mouse_event(flags, 0, 0, 0, 0) }
}

/// Drain queued messages of a single class for `hwnd`, repeating until
/// `PeekMessageA` returns 0. Mirrors the inline `PeekMessageA` loops in
/// the WA disassembly (`PM_REMOVE | PM_NOYIELD`).
#[inline]
unsafe fn drain_messages(hwnd: HWND, class: u32) {
    unsafe {
        let mut msg: MSG = core::mem::zeroed();
        while PeekMessageA(&mut msg, hwnd, class, class, PM_REMOVE | PM_NOYIELD) != 0 {}
    }
}

/// Body of [`get_message_proc`] — split out so the recursion-guard
/// clear-and-chain tail can be expressed as plain control flow.
unsafe fn get_message_proc_body(msg: *const MSG) {
    unsafe {
        let mode = InputHookMode::get();

        // Animated mode: either continue a pending synth pump, or start a
        // new one off an NC-button-down. The "pending" branch falls
        // through to the off/blocking drain logic; the "start new" branch
        // synthesises immediately and returns.
        if mode == InputHookMode::Animated {
            let pending = *(rb(va::G_INPUT_HOOK_FLAG_2DD7_MAYBE) as *const u8) != 0;
            if pending {
                let cap = GetCapture();
                if cap.is_null() || !FindWindowA(POPUP_MENU_CLASS.as_ptr(), ptr::null()).is_null() {
                    *(rb(va::G_INPUT_HOOK_FLAG_2DD7_MAYBE) as *mut u8) = 0;
                }
                // Fall through to the off/blocking drain branch below.
            } else {
                let m = (*msg).message;
                let nc_right: u8 = match m {
                    WM_NCLBUTTONDOWN => 0,
                    WM_NCRBUTTONDOWN => 1,
                    _ => return,
                };
                *(rb(G_GETMSG_NC_RIGHT) as *mut u8) = nc_right;
                *(rb(va::G_INPUT_HOOK_FLAG_2DD7_MAYBE) as *mut u8) = 1;
                let filter_select = *(rb(va::G_INPUT_HOOK_FILTER_SELECT_MAYBE) as *const u8);
                if filter_select == 0 {
                    synth_wheel_event();
                } else {
                    let pressed = (GetKeyState(VK_MBUTTON as i32) as i16) < 0;
                    *(rb(G_GETMSG_MBTN_PRESSED) as *mut u8) = pressed as u8;
                    synth_middle_button(pressed);
                }
                return;
            }
        }

        // Off / Blocking / Animated-with-pending: only act if blocking
        // input mode or a synth pump is in flight.
        let pending = *(rb(va::G_INPUT_HOOK_FLAG_2DD7_MAYBE) as *const u8) != 0;
        if mode != InputHookMode::Blocking && !pending {
            return;
        }

        let filter_select = *(rb(va::G_INPUT_HOOK_FILTER_SELECT_MAYBE) as *const u8);
        let m = (*msg).message;
        let matches_filter = if filter_select == 0 {
            m == WM_MOUSEWHEEL
        } else {
            (WM_MBUTTONDOWN..=WM_MBUTTONDBLCLK).contains(&m)
        };

        if matches_filter {
            pump_modal_or_session_frame();
            let ack = *(rb(G_GETMSG_STATE_ACK) as *const u8);
            if ack != 0 {
                *(rb(G_GETMSG_STATE_ACK) as *mut u8) = 0;
            } else {
                let new_pressed: u8 = (m != WM_MBUTTONUP) as u8;
                let prev_pressed = *(rb(G_GETMSG_MBTN_PRESSED) as *const u8);
                *(rb(G_GETMSG_MBTN_PRESSED) as *mut u8) = new_pressed;
                *(rb(G_GETMSG_STATE_ACK) as *mut u8) = (new_pressed != prev_pressed) as u8;
            }
        }

        let nc_right = *(rb(G_GETMSG_NC_RIGHT) as *const u8) != 0;
        if !nc_right {
            // No NC-button synth in flight — don't synthesise if the OS
            // already has queued mouse events; let the natural pump handle
            // them.
            let qstatus = GetQueueStatus(QS_MOUSE);
            if (qstatus >> 16) != 0 {
                return;
            }
            if filter_select == 0 {
                synth_wheel_event();
            } else {
                let pressed = *(rb(G_GETMSG_MBTN_PRESSED) as *const u8) != 0;
                synth_middle_button(pressed);
            }
        } else {
            // NC-button synth in flight — drain queued messages of the
            // synth's class first, then emit one final mouse_event.
            let hwnd = (*msg).hwnd;
            if filter_select == 0 {
                drain_messages(hwnd, WM_MOUSEWHEEL);
                synth_wheel_event();
            } else {
                let pressed = *(rb(G_GETMSG_MBTN_PRESSED) as *const u8) != 0;
                if pressed {
                    drain_messages(hwnd, WM_MBUTTONDOWN);
                    drain_messages(hwnd, WM_MBUTTONDBLCLK);
                } else {
                    drain_messages(hwnd, WM_MBUTTONUP);
                }
                synth_middle_button(pressed);
            }
        }
    }
}

/// Rust port of `Frontend::GetMessageProc` (0x004ED160).
///
/// `WH_GETMESSAGE` hook proc — synthesises `mouse_event` calls so the
/// engine can drain `WM_MOUSEWHEEL` or `WM_MBUTTON*` bursts while a
/// modal-dialog input grab is active.
///
/// Body only runs on the (`code == HC_ACTION`, `wParam == PM_REMOVE`)
/// edge of the hook — matches WA's `code == 0 && wparam == 1` test. A
/// recursion-guard byte (`g_GetMsgProc_Recursing`) prevents re-entry
/// while the body is dispatching synthesised input.
///
/// Always chains via `CallNextHookEx(g_FrontendMsgHook, ...)`.
///
/// Registered by [`install_input_hooks`]; the WA-side address
/// (`va::FRONTEND_GET_MESSAGE_PROC`) is install-trapped as a safety net
/// since no other code references it.
pub unsafe extern "system" fn get_message_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let recursing = *(rb(G_GETMSG_RECURSING) as *const u8) != 0;
        if !recursing && code == 0 && wparam == PM_REMOVE as WPARAM {
            *(rb(G_GETMSG_RECURSING) as *mut u8) = 1;
            get_message_proc_body(lparam as *const MSG);
            *(rb(G_GETMSG_RECURSING) as *mut u8) = 0;
        }
        let hook = *(rb(va::G_FRONTEND_MSG_HOOK) as *const HHOOK);
        CallNextHookEx(hook, code, wparam, lparam)
    }
}
