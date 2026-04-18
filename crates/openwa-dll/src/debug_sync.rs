//! Frame-level synchronization for debugging.
//!
//! Provides cooperative pause/resume at frame boundaries. The game thread
//! calls [`on_frame_start`] from the TurnManager hook; when paused, it blocks
//! on a Windows event until the debug server signals it to continue.
//!
//! All control functions are safe to call from any thread.

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::atomic::AtomicUsize;

use crate::log_line;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Threading::{
    CreateEventA, ResetEvent, SetEvent, WaitForSingleObject,
};

/// True when the game thread should block at the next frame boundary.
static PAUSED: AtomicBool = AtomicBool::new(false);

/// Frames remaining before auto-pausing (for `step N`). -1 = inactive.
static STEP_REMAINING: AtomicI32 = AtomicI32::new(-1);

/// Frame number to auto-pause at. -1 = no breakpoint.
static BREAK_FRAME: AtomicI32 = AtomicI32::new(-1);

/// Last frame number seen by on_frame_start.
static CURRENT_FRAME: AtomicI32 = AtomicI32::new(-1);

/// Manual-reset event handle stored as usize (HANDLE = *mut c_void).
static RESUME_EVENT: AtomicUsize = AtomicUsize::new(0);

fn event() -> HANDLE {
    RESUME_EVENT.load(Ordering::Relaxed) as HANDLE
}

/// Initialize the sync primitives. Call once from DLL init.
pub fn init() {
    let handle = unsafe { CreateEventA(core::ptr::null(), 1, 1, core::ptr::null()) };
    assert!(!handle.is_null(), "debug_sync: CreateEventA failed");
    RESUME_EVENT.store(handle as usize, Ordering::Relaxed);

    // Check for OPENWA_BREAK_FRAME env var
    if let Ok(val) = std::env::var("OPENWA_BREAK_FRAME")
        && let Ok(frame) = val.parse::<i32>()
    {
        BREAK_FRAME.store(frame, Ordering::Relaxed);
        let _ = log_line(&format!("[DebugSync] Breakpoint set at frame {frame}"));
    }
}

/// Called from TurnManager_ProcessFrame hook at the START of each frame.
/// Blocks the game thread if paused, breakpoint hit, or stepping completed.
pub fn on_frame_start(frame: i32) {
    CURRENT_FRAME.store(frame, Ordering::Relaxed);

    // Check breakpoint
    let bp = BREAK_FRAME.load(Ordering::Relaxed);
    if bp >= 0 && frame >= bp {
        BREAK_FRAME.store(-1, Ordering::Relaxed); // one-shot
        let _ = log_line(&format!("[DebugSync] Breakpoint hit at frame {frame}"));
        do_suspend();
    }

    // Check step counter
    let remaining = STEP_REMAINING.load(Ordering::Relaxed);
    if remaining > 0 {
        let new = STEP_REMAINING.fetch_sub(1, Ordering::Relaxed) - 1;
        if new <= 0 {
            STEP_REMAINING.store(-1, Ordering::Relaxed);
            let _ = log_line(&format!("[DebugSync] Step complete at frame {frame}"));
            do_suspend();
        }
    }

    // Block if paused
    if PAUSED.load(Ordering::Acquire) {
        unsafe {
            WaitForSingleObject(event(), 0xFFFFFFFF);
        }
    }
}

/// Pause the game at the next frame boundary.
pub fn suspend() {
    do_suspend();
    let _ = log_line(&format!(
        "[DebugSync] Suspended at frame {}",
        CURRENT_FRAME.load(Ordering::Relaxed)
    ));
}

/// Resume the game.
pub fn resume() {
    PAUSED.store(false, Ordering::Release);
    STEP_REMAINING.store(-1, Ordering::Relaxed);
    unsafe {
        SetEvent(event());
    }
    let _ = log_line("[DebugSync] Resumed");
}

/// Advance `count` frames, then pause again.
pub fn step(count: i32) {
    STEP_REMAINING.store(count.max(1), Ordering::Relaxed);
    PAUSED.store(false, Ordering::Release);
    unsafe {
        SetEvent(event());
    }
}

/// Set a frame breakpoint. -1 to clear.
pub fn set_breakpoint(frame: i32) {
    BREAK_FRAME.store(frame, Ordering::Relaxed);
}

/// Query current frame number.
pub fn current_frame() -> i32 {
    CURRENT_FRAME.load(Ordering::Relaxed)
}

/// Query pause state.
pub fn is_paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

/// Query current breakpoint. -1 = none.
pub fn breakpoint() -> i32 {
    BREAK_FRAME.load(Ordering::Relaxed)
}

// ── Internal ──

fn do_suspend() {
    PAUSED.store(true, Ordering::Release);
    unsafe {
        ResetEvent(event());
    }
}
