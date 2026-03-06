//! Configuration system hooks.
//!
//! Replaces WA.exe configuration functions with Rust implementations:
//! - Theme__GetFileSize (0x44BA80): theme file size query
//! - Theme__Load (0x44BB20): theme file read
//! - Theme__Save (0x44BBC0): theme file write
//! - Registry__DeleteKeyRecursive (0x4E4D10): recursive registry deletion
//! - Registry__CleanAll (0x4C90D0): full registry cleanup

use openwa_types::address::va;
use crate::log_line;
#[allow(unused_imports)]
use openwa_lib::rebase::rb;

const THEME_PATH: &str = "data\\current.thm";

// ============================================================
// Theme__GetFileSize replacement (0x44BA80)
// ============================================================

/// Rust replacement for Theme__GetFileSize.
/// cdecl() -> u32 (file length, or 0 if missing)
unsafe extern "cdecl" fn hook_theme_get_file_size() -> u32 {
    match std::fs::metadata(THEME_PATH) {
        Ok(m) => m.len() as u32,
        Err(_) => 0,
    }
}

// ============================================================
// Theme__Load replacement (0x44BB20)
// ============================================================

/// Rust replacement for Theme__Load.
/// stdcall(dest_buffer: *mut u8)
unsafe extern "stdcall" fn hook_theme_load(dest: u32) {
    match std::fs::read(THEME_PATH) {
        Ok(data) => {
            core::ptr::copy_nonoverlapping(data.as_ptr(), dest as *mut u8, data.len());
        }
        Err(_) => {
            show_error_message("ERROR: NO CURRENT.THM FILE FOUND");
        }
    }
}

// ============================================================
// Theme__Save replacement (0x44BBC0)
// ============================================================

/// Rust replacement for Theme__Save.
/// stdcall(buffer: *const u8, size: u32)
unsafe extern "stdcall" fn hook_theme_save(buffer: u32, size: u32) {
    let data = core::slice::from_raw_parts(buffer as *const u8, size as usize);
    if let Err(_) = std::fs::write(THEME_PATH, data) {
        show_error_message("ERROR: Could Not create CURRENT.THM File");
    }
}

/// Show an error message box, matching AfxMessageBox behavior.
fn show_error_message(msg: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut msg_buf: Vec<u8> = msg.bytes().collect();
    msg_buf.push(0);
    unsafe {
        MessageBoxA(core::ptr::null_mut(), msg_buf.as_ptr(), core::ptr::null(), MB_OK);
    }
}

// ============================================================
// Registry__DeleteKeyRecursive replacement (0x4E4D10)
// ============================================================

/// Rust replacement for Registry__DeleteKeyRecursive.
/// stdcall(hkey: HKEY, subkey: *const u8) -> i32
unsafe extern "stdcall" fn hook_delete_key_recursive(hkey: u32, subkey: u32) -> i32 {
    use windows_sys::Win32::System::Registry::HKEY;

    let c_subkey = std::ffi::CStr::from_ptr(subkey as *const i8);
    let subkey_str = c_subkey.to_string_lossy();

    let _ = log_line(&format!("[Config] DeleteKeyRecursive: {subkey_str}"));

    let result = openwa_lib::wa::registry::delete_key_recursive(
        hkey as usize as HKEY,
        &subkey_str,
    );

    let _ = log_line(&format!("[Config] DeleteKeyRecursive result: {result}"));
    result as i32
}

// ============================================================
// Registry__CleanAll replacement (0x4C90D0)
// ============================================================

/// Rust replacement for Registry__CleanAll.
/// stdcall(struct_ptr: u32)
unsafe extern "stdcall" fn hook_registry_clean_all(struct_ptr: u32) {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

    let _ = log_line("[Config] CleanAll: deleting registry sections");

    let sections = [
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\Data",
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\Options",
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\ExportVideo",
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\VSyncAssist",
    ];

    for section in &sections {
        openwa_lib::wa::registry::delete_key_recursive(HKEY_CURRENT_USER, section);
    }

    // Clear the NetSettings INI section
    extern "system" {
        fn WriteProfileSectionA(app_name: *const u8, string: *const u8) -> i32;
    }
    WriteProfileSectionA(b"NetSettings\0".as_ptr(), b"\0".as_ptr());

    // Set struct_ptr + 0xE0 = 0
    *((struct_ptr + 0xE0) as *mut u8) = 0;

    let _ = log_line("[Config] CleanAll completed");
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = crate::hook::install(
            "Theme__GetFileSize",
            va::THEME_GET_FILE_SIZE,
            hook_theme_get_file_size as *const (),
        )?;

        let _ = crate::hook::install(
            "Theme__Load",
            va::THEME_LOAD,
            hook_theme_load as *const (),
        )?;

        let _ = crate::hook::install(
            "Theme__Save",
            va::THEME_SAVE,
            hook_theme_save as *const (),
        )?;

        let _ = crate::hook::install(
            "Registry__DeleteKeyRecursive",
            va::REGISTRY_DELETE_KEY_RECURSIVE,
            hook_delete_key_recursive as *const (),
        )?;

        let _ = crate::hook::install(
            "Registry__CleanAll",
            va::REGISTRY_CLEAN_ALL,
            hook_registry_clean_all as *const (),
        )?;
    }

    Ok(())
}
