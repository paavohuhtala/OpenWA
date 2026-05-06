//! Hook wiring for Scheme__* functions.
//!
//! Thin shim — all logic lives in `openwa_game::engine::scheme_ops`.
//! This file holds trampoline storage, hook entry stubs that capture register
//! params, and `install()`.

use std::ffi::CStr;
use std::sync::atomic::{AtomicU32, Ordering};

use openwa_game::address::va;
use openwa_game::engine::scheme_ops::{self, OriginalReadFile, OriginalSaveFile};

use crate::hook::{self, usercall_trampoline};
use crate::log_line;

// ─── Trampoline storage ─────────────────────────────────────────────────────

static ORIG_SCHEME_READ_FILE: AtomicU32 = AtomicU32::new(0);
static ORIG_SCHEME_SAVE_FILE: AtomicU32 = AtomicU32::new(0);
static ORIG_LOAD_NUMBERED: AtomicU32 = AtomicU32::new(0);

unsafe extern "stdcall" fn original_read_file(
    dest: u32,
    path: u32,
    flag: u32,
    out_ptr: u32,
) -> u32 {
    unsafe {
        let orig: OriginalReadFile =
            core::mem::transmute(ORIG_SCHEME_READ_FILE.load(Ordering::Relaxed));
        orig(dest, path, flag, out_ptr)
    }
}

unsafe extern "fastcall" fn original_save_file(this: u32, edx: u32, name: u32, flag: u32) -> u32 {
    unsafe {
        let orig: OriginalSaveFile =
            core::mem::transmute(ORIG_SCHEME_SAVE_FILE.load(Ordering::Relaxed));
        orig(this, edx, name, flag)
    }
}

// ─── Hook entry stubs ───────────────────────────────────────────────────────

unsafe extern "stdcall" fn hook_read_file(
    dest_struct: u32,
    file_path: u32,
    flag: u32,
    out_ptr: u32,
) -> u32 {
    unsafe { scheme_ops::read_file(dest_struct, file_path, flag, out_ptr, original_read_file) }
}

usercall_trampoline!(fn trampoline_validate_ext_opts;
    impl_fn = scheme_ops::validate_extended_options;
    reg = eax);

unsafe extern "stdcall" fn hook_file_exists(name: u32) -> u32 {
    unsafe { scheme_ops::file_exists(name) }
}

usercall_trampoline!(fn trampoline_detect_version;
    impl_fn = scheme_ops::detect_version;
    reg = esi; stack_params = 1; ret_bytes = "0x4");

unsafe extern "fastcall" fn hook_save_file(this: u32, _edx: u32, name: u32, flag: u32) -> u32 {
    unsafe { scheme_ops::save_file(this, name, flag, original_save_file) }
}

unsafe extern "fastcall" fn hook_init_from_data(
    _ecx: u32,
    src_data: u32,
    dest: u32,
    name_cstring: u32,
) {
    unsafe { scheme_ops::init_from_data(src_data, dest, name_cstring) }
}

unsafe extern "stdcall" fn hook_check_weapon_limits() -> u32 {
    unsafe { scheme_ops::check_weapon_limits() }
}

unsafe extern "stdcall" fn hook_scan_directory(cstring_param: u32) {
    unsafe { scheme_ops::scan_directory(cstring_param) }
}

unsafe extern "stdcall" fn hook_extract_builtins() {
    unsafe { scheme_ops::extract_builtins() }
}

/// Logging sentinel for Scheme__LoadNumbered (0x4D4E00) — believed dead code.
unsafe extern "stdcall" fn hook_load_numbered(name: u32) -> u32 {
    unsafe {
        let c_name = CStr::from_ptr(name as *const i8).to_str().unwrap_or("???");
        let _ = log_line(&format!(
            "[Scheme] WARNING: LoadNumbered called (believed dead code): {c_name}"
        ));
        let orig: unsafe extern "stdcall" fn(u32) -> u32 =
            core::mem::transmute(ORIG_LOAD_NUMBERED.load(Ordering::Relaxed));
        orig(name)
    }
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        let trampoline = hook::install(
            "Scheme__ReadFile",
            va::SCHEME_READ_FILE,
            hook_read_file as *const (),
        )?;
        ORIG_SCHEME_READ_FILE.store(trampoline as u32, Ordering::Relaxed);

        hook::install(
            "Scheme__ValidateExtendedOptions",
            va::SCHEME_VALIDATE_EXTENDED_OPTIONS,
            trampoline_validate_ext_opts as *const (),
        )?;

        hook::install(
            "Scheme__FileExists",
            va::SCHEME_FILE_EXISTS,
            hook_file_exists as *const (),
        )?;

        hook::install(
            "Scheme__CheckWeaponLimits",
            va::SCHEME_CHECK_WEAPON_LIMITS,
            hook_check_weapon_limits as *const (),
        )?;

        hook::install(
            "Scheme__DetectVersion",
            va::SCHEME_DETECT_VERSION,
            trampoline_detect_version as *const (),
        )?;

        let trampoline_save = hook::install(
            "Scheme__SaveFile",
            va::SCHEME_SAVE_FILE,
            hook_save_file as *const (),
        )?;
        ORIG_SCHEME_SAVE_FILE.store(trampoline_save as u32, Ordering::Relaxed);

        hook::install(
            "Scheme__InitFromData",
            va::SCHEME_INIT_FROM_DATA,
            hook_init_from_data as *const (),
        )?;

        hook::install(
            "Scheme__ScanDirectory",
            va::SCHEME_SCAN_DIRECTORY,
            hook_scan_directory as *const (),
        )?;

        hook::install(
            "Scheme__ExtractBuiltins",
            va::SCHEME_EXTRACT_BUILTINS,
            hook_extract_builtins as *const (),
        )?;

        let trampoline_load = hook::install(
            "Scheme__LoadNumbered",
            va::SCHEME_FILE_EXISTS_NUMBERED,
            hook_load_numbered as *const (),
        )?;
        ORIG_LOAD_NUMBERED.store(trampoline_load as u32, Ordering::Relaxed);
    }

    Ok(())
}
