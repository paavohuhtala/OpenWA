//! Cross-thread dispatch onto WA's main thread.
//!
//! Tools that run on background threads (debug UI, custom frontend) but
//! need to invoke WA APIs with main-thread affinity (rendering, DirectInput
//! acquisition, the game session main loop) schedule a callback here and
//! the main thread drains it on its next message pump.
//!
//! Mechanism: a single `AtomicPtr` holds at most one pending callback.
//! Drained on every Win32 `WH_GETMESSAGE` retrieval on WA's main thread.
//! The hook is installed lazily on the first [`schedule`] call once WA's
//! main window exists — DllMain runs on the injection thread, not the
//! main thread, so we can't install at DLL load time.
//!
//! The callback runs synchronously on the main thread. It may block
//! indefinitely (e.g. for a full game session) — the main thread is
//! inside MFC's message pump when we drain, so blocking there is the
//! same context as WA's own dialog handlers.

use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetWindowThreadProcessId, HHOOK, SetWindowsHookExA, UnhookWindowsHookEx,
    WH_GETMESSAGE,
};

use crate::address::va;
use crate::rebase::rb;

static PENDING: AtomicPtr<()> = AtomicPtr::new(ptr::null_mut());
static DRAIN_HOOK: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(ptr::null_mut());

/// Schedule `cb` to run once on the next main-thread message pump.
///
/// Returns `true` if a previous pending callback was overwritten.
pub fn schedule(cb: extern "C" fn()) -> bool {
    let prev = PENDING.swap(cb as *mut (), Ordering::AcqRel);
    ensure_hook_installed();
    !prev.is_null()
}

/// Drain the pending callback (if any) and run it on the calling thread.
/// Called from the WH_GETMESSAGE hook installed by [`ensure_hook_installed`],
/// and also (defensively) from WA's own frontend input-mode hooks.
pub fn try_run_pending() {
    let p = PENDING.swap(ptr::null_mut(), Ordering::AcqRel);
    if !p.is_null() {
        unsafe {
            let cb: extern "C" fn() = core::mem::transmute(p);
            cb();
        }
    }
}

unsafe extern "system" fn drain_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    try_run_pending();
    let h = DRAIN_HOOK.load(Ordering::Acquire) as HHOOK;
    unsafe { CallNextHookEx(h, code, wparam, lparam) }
}

/// Lazily install our WH_GETMESSAGE drain hook on WA's main thread.
/// No-op if already installed, or if WA's frontend window isn't up yet.
fn ensure_hook_installed() {
    if !DRAIN_HOOK.load(Ordering::Acquire).is_null() {
        return;
    }

    let hwnd: HWND = unsafe { *(rb(va::G_FRONTEND_HWND) as *const HWND) };
    if hwnd.is_null() {
        return;
    }

    let tid = unsafe { GetWindowThreadProcessId(hwnd, ptr::null_mut()) };
    if tid == 0 {
        return;
    }

    // Same-process per-thread hook — hinstance can be null.
    let h =
        unsafe { SetWindowsHookExA(WH_GETMESSAGE, Some(drain_hook_proc), ptr::null_mut(), tid) };
    if h.is_null() {
        return;
    }

    if DRAIN_HOOK
        .compare_exchange(
            ptr::null_mut(),
            h as *mut core::ffi::c_void,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        // Lost the race against another caller — unhook ours.
        unsafe {
            UnhookWindowsHookEx(h);
        }
    }
}
