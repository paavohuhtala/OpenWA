//! Scheme file (.wsc) hook.
//!
//! Hooks Scheme__ReadFile (0x4D3890) to log when schemes are loaded.
//! Currently a passthrough — calls the original and logs the result.
//! Future: replace the file I/O with our Rust SchemeFile parser.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use minhook::MinHook;

use crate::log_line;
use crate::rebase::rb;
use openwa_types::address::va;

/// Trampoline to original Scheme__ReadFile.
static ORIG_SCHEME_READ_FILE: AtomicU32 = AtomicU32::new(0);

/// Hook for Scheme__ReadFile (0x4D3890).
/// stdcall(dest_struct, file_path, flag, out_ptr) -> u32, RET 0x10
///
/// - dest_struct: pointer to destination scheme struct (payload written to +0x14)
/// - file_path: scheme file path string
/// - flag: boolean flag (passed to error display)
/// - out_ptr: optional output byte pointer
/// - returns: 0 on success, 0xFFFFFFFF on failure
unsafe extern "stdcall" fn hook_scheme_read_file(
    dest_struct: u32,
    file_path: u32,
    flag: u32,
    out_ptr: u32,
) -> u32 {
    // Read the scheme name from the file_path pointer
    let name = if file_path != 0 {
        let cstr = std::ffi::CStr::from_ptr(file_path as *const i8);
        cstr.to_string_lossy().into_owned()
    } else {
        "<null>".to_string()
    };

    // Call original
    let orig: unsafe extern "stdcall" fn(u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_SCHEME_READ_FILE.load(Ordering::Relaxed));
    let result = orig(dest_struct, file_path, flag, out_ptr);

    // Log the result
    if result == 0 {
        // Read the version byte from dest_struct+0x14 (first 5 bytes are the header)
        // Actually the header is consumed during read; the payload starts at +0x14
        let _ = log_line(&format!(
            "[Scheme] Loaded: {name} -> dest=0x{dest_struct:08X}"
        ));
    } else {
        let _ = log_line(&format!(
            "[Scheme] FAILED to load: {name} (result=0x{result:08X})"
        ));
    }

    result
}

pub fn install() -> Result<(), String> {
    unsafe {
        let target = rb(va::SCHEME_READ_FILE) as *mut c_void;
        let detour = hook_scheme_read_file as *const () as *mut c_void;

        let trampoline = MinHook::create_hook(target, detour)
            .map_err(|e| format!("MinHook create_hook failed for Scheme__ReadFile: {e}"))?;

        MinHook::enable_hook(target)
            .map_err(|e| format!("MinHook enable_hook failed for Scheme__ReadFile: {e}"))?;

        ORIG_SCHEME_READ_FILE.store(trampoline as u32, Ordering::Relaxed);

        let _ = log_line(&format!(
            "  [HOOK] Scheme__ReadFile: target 0x{:08X}, trampoline 0x{:08X}",
            target as u32, trampoline as u32
        ));
    }

    Ok(())
}
