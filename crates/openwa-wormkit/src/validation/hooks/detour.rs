//! MinHook-based inline hooking for constructors (passthrough).

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use minhook::MinHook;

use openwa_core::rebase::rb;
use openwa_core::address::va;

use super::log_validation;

unsafe fn install_hook(ghidra_addr: u32, detour_fn: *const (), name: &str) -> Result<u32, String> {
    let target = rb(ghidra_addr) as *mut c_void;
    let detour = detour_fn as *mut c_void;

    let trampoline = MinHook::create_hook(target, detour)
        .map_err(|e| format!("MinHook create_hook failed for {name}: {e}"))?;

    MinHook::enable_hook(target)
        .map_err(|e| format!("MinHook enable_hook failed for {name}: {e}"))?;

    let trampoline_addr = trampoline as u32;
    let _ = log_validation(&format!(
        "  [MH] {name}: target 0x{:08X} (ghidra 0x{ghidra_addr:08X}), trampoline 0x{trampoline_addr:08X}",
        target as u32
    ));

    Ok(trampoline_addr)
}

static ORIG_CTASK_CONSTRUCTOR: AtomicU32 = AtomicU32::new(0);

unsafe extern "stdcall" fn hook_ctask_constructor(this: u32, parent: u32, ddgame: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_CTASK_CONSTRUCTOR.load(Ordering::Relaxed));
    orig(this, parent, ddgame)
}

static ORIG_CGAMETASK_CONSTRUCTOR: AtomicU32 = AtomicU32::new(0);

unsafe extern "stdcall" fn hook_cgametask_constructor(this: u32, parent: u32, param3: u32, param4: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_CGAMETASK_CONSTRUCTOR.load(Ordering::Relaxed));
    orig(this, parent, param3, param4)
}

pub fn install() -> Result<(), String> {
    unsafe {
        let trampoline = install_hook(
            va::CTASK_CONSTRUCTOR,
            hook_ctask_constructor as *const (),
            "CTask::Constructor",
        )?;
        ORIG_CTASK_CONSTRUCTOR.store(trampoline, Ordering::Relaxed);

        let trampoline = install_hook(
            va::CGAMETASK_CONSTRUCTOR,
            hook_cgametask_constructor as *const (),
            "CGameTask::Constructor",
        )?;
        ORIG_CGAMETASK_CONSTRUCTOR.store(trampoline, Ordering::Relaxed);
    }

    let _ = log_validation("  2 inline hooks installed via MinHook");
    Ok(())
}
