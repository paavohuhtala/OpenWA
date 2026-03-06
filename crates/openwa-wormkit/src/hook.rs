//! MinHook helper for installing inline hooks on WA.exe functions.

use std::ffi::c_void;

use minhook::MinHook;

use crate::log_line;
use crate::rebase::rb;

/// Install a MinHook inline hook on a WA.exe function.
///
/// - `name`: human-readable name for log/error messages
/// - `ghidra_addr`: Ghidra VA of the target function (rebased automatically)
/// - `detour`: replacement function pointer
///
/// Returns the trampoline pointer (for calling the original).
/// Caller can store it in an `AtomicU32` if needed, or discard with `let _ =`.
pub unsafe fn install(
    name: &str,
    ghidra_addr: u32,
    detour: *const (),
) -> Result<*mut c_void, String> {
    let target = rb(ghidra_addr) as *mut c_void;
    let detour = detour as *mut c_void;

    let trampoline = MinHook::create_hook(target, detour)
        .map_err(|e| format!("MinHook create_hook failed for {name}: {e}"))?;

    MinHook::enable_hook(target)
        .map_err(|e| format!("MinHook enable_hook failed for {name}: {e}"))?;

    let _ = log_line(&format!(
        "  [REPLACE] {name}: target 0x{:08X}, trampoline 0x{:08X}",
        target as u32, trampoline as u32
    ));

    Ok(trampoline)
}
