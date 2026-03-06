//! Configuration system hooks.
//!
//! Replaces WA.exe configuration functions with Rust implementations:
//! - Theme__GetFileSize (0x44BA80): theme file size query
//! - Theme__Load (0x44BB20): theme file read
//! - Theme__Save (0x44BBC0): theme file write

use openwa_types::address::va;

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
    }

    Ok(())
}
