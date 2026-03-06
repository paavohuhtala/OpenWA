//! Scheme file (.wsc) hooks.
//!
//! Replaces several WA.exe Scheme__ functions with Rust implementations:
//! - Scheme__ReadFile (0x4D3890): full Rust file I/O using SchemeFile parser
//! - Scheme__ValidateExtendedOptions (0x4D5110): pure Rust validation
//! - Scheme__FileExists (0x4D4CD0): Rust path check

use std::ffi::{c_void, CStr};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use minhook::MinHook;

use crate::log_line;
use crate::rebase::rb;
use openwa_types::address::va;
use openwa_types::scheme::{
    ExtendedOptions, SchemeFile, SchemeVersion, EXTENDED_OPTIONS_DEFAULTS, EXTENDED_OPTIONS_SIZE,
    SCHEME_PAYLOAD_V1, SCHEME_PAYLOAD_V2, SCHEME_PAYLOAD_V3,
};

// ============================================================
// Scheme__ReadFile replacement (0x4D3890)
// ============================================================

/// Trampoline to original Scheme__ReadFile (for fallback on flag/out_ptr path).
static ORIG_SCHEME_READ_FILE: AtomicU32 = AtomicU32::new(0);

/// Dest struct offsets (WA runtime layout).
const DEST_FLAG: usize = 0x04;
const DEST_INDEX: usize = 0x08;
const DEST_PAYLOAD: usize = 0x14;
/// Offset within payload where super weapons start (V1 payload ends here).
const PAYLOAD_SUPER_WEAPONS: usize = SCHEME_PAYLOAD_V1; // 0xD8
/// Size of super weapons region to zero for V1 schemes.
const SUPER_WEAPONS_SIZE: usize = SCHEME_PAYLOAD_V2 - SCHEME_PAYLOAD_V1; // 0x4C = 76
/// Offset within payload where extended options start.
const PAYLOAD_EXTENDED: usize = SCHEME_PAYLOAD_V2; // 0x124

/// Write a parsed SchemeFile into the WA dest struct at the correct offsets.
///
/// Replicates the memory writes from Scheme__ReadFile:
/// - V1: payload(0xD8) + zero super weapons(0x4C) + ROM defaults(0x6E)
/// - V2: payload(0x124) + ROM defaults(0x6E)
/// - V3: payload(0x192), with validation fallback to ROM defaults
unsafe fn write_scheme_to_dest(scheme: &SchemeFile, dest: u32) {
    let payload_ptr = (dest as usize + DEST_PAYLOAD) as *mut u8;

    match scheme.version {
        SchemeVersion::V1 => {
            // Copy V1 payload (216 bytes)
            core::ptr::copy_nonoverlapping(
                scheme.payload.as_ptr(),
                payload_ptr,
                SCHEME_PAYLOAD_V1,
            );
            // Zero the super weapons region (76 bytes)
            core::ptr::write_bytes(
                payload_ptr.add(PAYLOAD_SUPER_WEAPONS),
                0,
                SUPER_WEAPONS_SIZE,
            );
            // Copy V3 defaults for extended options (110 bytes)
            core::ptr::copy_nonoverlapping(
                EXTENDED_OPTIONS_DEFAULTS.as_ptr(),
                payload_ptr.add(PAYLOAD_EXTENDED),
                EXTENDED_OPTIONS_SIZE,
            );
        }
        SchemeVersion::V2 => {
            // Copy V2 payload (292 bytes)
            core::ptr::copy_nonoverlapping(
                scheme.payload.as_ptr(),
                payload_ptr,
                SCHEME_PAYLOAD_V2,
            );
            // Copy V3 defaults for extended options (110 bytes)
            core::ptr::copy_nonoverlapping(
                EXTENDED_OPTIONS_DEFAULTS.as_ptr(),
                payload_ptr.add(PAYLOAD_EXTENDED),
                EXTENDED_OPTIONS_SIZE,
            );
        }
        SchemeVersion::V3 => {
            // Copy full V3 payload (402 bytes)
            core::ptr::copy_nonoverlapping(
                scheme.payload.as_ptr(),
                payload_ptr,
                SCHEME_PAYLOAD_V3,
            );
            // Validate extended options; if invalid, replace with defaults
            let ext_bytes = &scheme.payload[PAYLOAD_EXTENDED..];
            if !ExtendedOptions::validate_bytes(ext_bytes) {
                core::ptr::copy_nonoverlapping(
                    EXTENDED_OPTIONS_DEFAULTS.as_ptr(),
                    payload_ptr.add(PAYLOAD_EXTENDED),
                    EXTENDED_OPTIONS_SIZE,
                );
            }
        }
    }

    // Set scheme index to 0xFFFFFFFF (unassigned)
    *((dest as usize + DEST_INDEX) as *mut u32) = 0xFFFF_FFFF;
}

/// Rust replacement for Scheme__ReadFile (0x4D3890).
/// stdcall(dest_struct, file_path, flag, out_ptr) -> u32, RET 0x10
///
/// out_ptr is an output bool: 0 = no gameplay modifiers applied, 1 = modifiers applied.
/// flag controls whether CheckWeaponLimits is called after loading.
///
/// TODO: Gameplay modifier globals (~20 fields at 0x88DBxx) are not yet ported.
/// These are applied in online lobbies; for now we always report "no modifiers applied".
unsafe extern "stdcall" fn hook_scheme_read_file(
    dest_struct: u32,
    file_path: u32,
    _flag: u32,
    out_ptr: u32,
) -> u32 {
    // Initialize out_ptr to 0 (no modifiers applied), matching original behavior
    if out_ptr != 0 {
        *(out_ptr as *mut u8) = 0;
    }

    // Read the path string
    let name = if file_path != 0 {
        match CStr::from_ptr(file_path as *const i8).to_str() {
            Ok(s) => s,
            Err(_) => {
                let _ = log_line("[Scheme] ReadFile: invalid UTF-8 path, delegating to original");
                return call_original_read_file(dest_struct, file_path, _flag, out_ptr);
            }
        }
    } else {
        let _ = log_line("[Scheme] ReadFile: null path");
        return 0xFFFF_FFFF;
    };

    // Use Rust parser for file I/O
    let path = Path::new(name);
    match SchemeFile::from_file(path) {
        Ok(scheme) => {
            write_scheme_to_dest(&scheme, dest_struct);

            // Set flag byte to 0 and scheme index to 0xFFFFFFFF
            *((dest_struct as usize + DEST_FLAG) as *mut u8) = 0;

            let _ = log_line(&format!(
                "[Scheme] ReadFile OK (Rust): {name} -> {:?}, dest=0x{dest_struct:08X}",
                scheme.version
            ));
            0
        }
        Err(e) => {
            let _ = log_line(&format!("[Scheme] ReadFile FAILED (Rust): {name}: {e}"));
            0xFFFF_FFFF
        }
    }
}

/// Call the original Scheme__ReadFile via trampoline.
#[inline]
unsafe fn call_original_read_file(dest: u32, path: u32, flag: u32, out_ptr: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_SCHEME_READ_FILE.load(Ordering::Relaxed));
    orig(dest, path, flag, out_ptr)
}

// ============================================================
// Scheme__ValidateExtendedOptions replacement (0x4D5110)
// ============================================================

/// Naked trampoline for Scheme__ValidateExtendedOptions.
///
/// WA calls this with EAX = pointer to 110-byte extended options data.
/// Plain RET (no stack cleanup). Returns EAX: 0 = valid, 1 = invalid.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_validate_ext_opts() {
    core::arch::naked_asm!(
        "push eax",         // push options pointer as arg
        "call {impl_fn}",  // cdecl call
        "add esp, 4",      // clean up our push
        "ret",              // plain ret (no stack params to clean)
        impl_fn = sym validate_extended_options_impl,
    );
}

/// Rust implementation called by the naked trampoline.
unsafe extern "cdecl" fn validate_extended_options_impl(options_ptr: u32) -> u32 {
    let bytes = core::slice::from_raw_parts(options_ptr as *const u8, EXTENDED_OPTIONS_SIZE);
    if ExtendedOptions::validate_bytes(bytes) {
        0 // valid
    } else {
        1 // invalid
    }
}

// ============================================================
// Scheme__FileExists replacement (0x4D4CD0)
// ============================================================

/// Rust replacement for Scheme__FileExists (0x4D4CD0).
/// stdcall(name) -> u32 (0 = not found, 1 = found), RET 0x4
///
/// Original: formats "User\Schemes\%s.wsc", opens with CFile.
/// Rust: uses std::path::Path::exists().
unsafe extern "stdcall" fn hook_scheme_file_exists(name: u32) -> u32 {
    if name == 0 {
        return 0;
    }
    let c_name = CStr::from_ptr(name as *const i8);
    let name_str = c_name.to_string_lossy();
    let path = format!("User\\Schemes\\{name_str}.wsc");
    if Path::new(&path).exists() {
        1
    } else {
        0
    }
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        // Hook Scheme__ReadFile (0x4D3890) — full Rust replacement
        {
            let target = rb(va::SCHEME_READ_FILE) as *mut c_void;
            let detour = hook_scheme_read_file as *const () as *mut c_void;

            let trampoline = MinHook::create_hook(target, detour)
                .map_err(|e| format!("MinHook create_hook failed for Scheme__ReadFile: {e}"))?;

            MinHook::enable_hook(target)
                .map_err(|e| format!("MinHook enable_hook failed for Scheme__ReadFile: {e}"))?;

            ORIG_SCHEME_READ_FILE.store(trampoline as u32, Ordering::Relaxed);

            let _ = log_line(&format!(
                "  [REPLACE] Scheme__ReadFile: target 0x{:08X}, trampoline 0x{:08X}",
                target as u32, trampoline as u32
            ));
        }

        // Hook Scheme__ValidateExtendedOptions (0x4D5110) — full Rust replacement
        {
            let target = rb(va::SCHEME_VALIDATE_EXTENDED_OPTIONS) as *mut c_void;
            let detour = trampoline_validate_ext_opts as *const () as *mut c_void;

            MinHook::create_hook(target, detour).map_err(|e| {
                format!("MinHook create_hook failed for ValidateExtendedOptions: {e}")
            })?;

            MinHook::enable_hook(target).map_err(|e| {
                format!("MinHook enable_hook failed for ValidateExtendedOptions: {e}")
            })?;

            let _ = log_line(&format!(
                "  [REPLACE] Scheme__ValidateExtendedOptions: target 0x{:08X}",
                target as u32
            ));
        }

        // Hook Scheme__FileExists (0x4D4CD0) — full Rust replacement
        {
            let target = rb(va::SCHEME_FILE_EXISTS) as *mut c_void;
            let detour = hook_scheme_file_exists as *const () as *mut c_void;

            MinHook::create_hook(target, detour).map_err(|e| {
                format!("MinHook create_hook failed for Scheme__FileExists: {e}")
            })?;

            MinHook::enable_hook(target).map_err(|e| {
                format!("MinHook enable_hook failed for Scheme__FileExists: {e}")
            })?;

            let _ = log_line(&format!(
                "  [REPLACE] Scheme__FileExists: target 0x{:08X}",
                target as u32
            ));
        }
    }

    Ok(())
}
