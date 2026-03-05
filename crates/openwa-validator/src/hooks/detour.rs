//! MinHook-based inline hooking for arbitrary functions.
//!
//! Uses MinHook to create trampolines that correctly handle SEH prologues
//! (which retour-rs could not). This allows hooking constructors and free
//! functions that aren't in vtables.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use minhook::MinHook;

use crate::{log_line, rb};
use openwa_types::address::va;

/// Install an inline hook at a Ghidra address, redirecting to `detour_fn`.
/// Returns the trampoline pointer for calling the original.
unsafe fn install_hook(ghidra_addr: u32, detour_fn: *const (), name: &str) -> Result<u32, String> {
    let target = rb(ghidra_addr) as *mut c_void;
    let detour = detour_fn as *mut c_void;

    let trampoline = MinHook::create_hook(target, detour)
        .map_err(|e| format!("MinHook create_hook failed for {name}: {e}"))?;

    MinHook::enable_hook(target)
        .map_err(|e| format!("MinHook enable_hook failed for {name}: {e}"))?;

    let trampoline_addr = trampoline as u32;
    let _ = log_line(&format!(
        "  [MH] {name}: target 0x{:08X} (ghidra 0x{ghidra_addr:08X}), trampoline 0x{trampoline_addr:08X}",
        target as u32
    ));

    Ok(trampoline_addr)
}

// ---------------------------------------------------------------------------
// FrontendChangeScreen hook
// ---------------------------------------------------------------------------

static ORIG_FRONTEND_CHANGE_SCREEN: AtomicU32 = AtomicU32::new(0);

/// Hook for FrontendChangeScreen (0x447A20).
/// Called on every menu screen transition. Logs the screen ID.
///
/// RET 0x4 → stdcall with 1 param (screen_id). ESI = dialog this (implicit).
unsafe extern "stdcall" fn hook_frontend_change_screen(screen_id: u32) {
    let _ = log_line(&format!("[HOOK] FrontendChangeScreen(screen_id={screen_id})"));
    let orig: unsafe extern "stdcall" fn(u32) =
        core::mem::transmute(ORIG_FRONTEND_CHANGE_SCREEN.load(Ordering::Relaxed));
    orig(screen_id);
}

// ---------------------------------------------------------------------------
// CTask::Constructor hook
// ---------------------------------------------------------------------------

static ORIG_CTASK_CONSTRUCTOR: AtomicU32 = AtomicU32::new(0);

/// Hook for CTask::Constructor (0x5625A0).
/// stdcall(this, parent, ddgame) -> this
/// 3rd param is the DDGame pointer, stored at this+0x2C.
unsafe extern "stdcall" fn hook_ctask_constructor(this: u32, parent: u32, ddgame: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_CTASK_CONSTRUCTOR.load(Ordering::Relaxed));
    let result = orig(this, parent, ddgame);
    // Log after calling original (so vtable is set)
    let _ = log_line(&format!(
        "[HOOK] CTask::ctor(this=0x{this:08X}, parent=0x{parent:08X}, ddgame=0x{ddgame:08X}) -> 0x{result:08X}"
    ));
    result
}

// ---------------------------------------------------------------------------
// CGameTask::Constructor hook
// ---------------------------------------------------------------------------

static ORIG_CGAMETASK_CONSTRUCTOR: AtomicU32 = AtomicU32::new(0);

/// Hook for CGameTask::Constructor (0x4FED50).
/// RET 0x10 → stdcall with 4 params: (this, parent, param3, param4)
/// param3/param4 are stored at this+0x30/this+0x38 (likely initial position)
unsafe extern "stdcall" fn hook_cgametask_constructor(this: u32, parent: u32, param3: u32, param4: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_CGAMETASK_CONSTRUCTOR.load(Ordering::Relaxed));
    let result = orig(this, parent, param3, param4);
    let _ = log_line(&format!(
        "[HOOK] CGameTask::ctor(this=0x{this:08X}, parent=0x{parent:08X}) -> 0x{result:08X}"
    ));
    result
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

pub fn install() -> Result<(), String> {
    unsafe {
        // FrontendChangeScreen — called on every menu navigation
        let trampoline = install_hook(
            va::FRONTEND_CHANGE_SCREEN,
            hook_frontend_change_screen as *const (),
            "FrontendChangeScreen",
        )?;
        ORIG_FRONTEND_CHANGE_SCREEN.store(trampoline, Ordering::Relaxed);

        // CTask::Constructor — base class constructor
        let trampoline = install_hook(
            va::CTASK_CONSTRUCTOR,
            hook_ctask_constructor as *const (),
            "CTask::Constructor",
        )?;
        ORIG_CTASK_CONSTRUCTOR.store(trampoline, Ordering::Relaxed);

        // CGameTask::Constructor — game entity constructor
        let trampoline = install_hook(
            va::CGAMETASK_CONSTRUCTOR,
            hook_cgametask_constructor as *const (),
            "CGameTask::Constructor",
        )?;
        ORIG_CGAMETASK_CONSTRUCTOR.store(trampoline, Ordering::Relaxed);
    }

    let _ = log_line("  3 inline hooks installed via MinHook");
    Ok(())
}
