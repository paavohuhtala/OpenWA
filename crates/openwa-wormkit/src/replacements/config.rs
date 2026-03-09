//! Configuration system hooks.
//!
//! Replaces WA.exe configuration functions with Rust implementations:
//! - Theme__GetFileSize (0x44BA80): theme file size query
//! - Theme__Load (0x44BB20): theme file read
//! - Theme__Save (0x44BBC0): theme file write
//! - Registry__DeleteKeyRecursive (0x4E4D10): recursive registry deletion
//! - Registry__CleanAll (0x4C90D0): full registry cleanup
//! - GameInfo__LoadOptions (0x460AC0): game options from registry
//! - Options__GetCrashReportURL (0x5A63F0): crash report URL from registry

use openwa_core::address::va;
use crate::log_line;
use openwa_core::rebase::rb;

const THEME_PATH: &str = "data\\current.thm";
const CLEAN_ALL_FLAG_OFFSET: usize = 0xE0;
const CRASH_REPORT_URL_BUF_SIZE: usize = 0x400;
/// Max CameraUnlockMouseSpeed before squaring — sqrt(2^31) ≈ 46340, prevents overflow.
const CAMERA_UNLOCK_SPEED_MAX: u32 = 0xB504;

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

    let result = openwa_core::wa::registry::delete_key_recursive(
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
        openwa_core::wa::registry::delete_key_recursive(HKEY_CURRENT_USER, section);
    }

    // Clear the NetSettings INI section
    extern "system" {
        fn WriteProfileSectionA(app_name: *const u8, string: *const u8) -> i32;
    }
    WriteProfileSectionA(b"NetSettings\0".as_ptr(), b"\0".as_ptr());

    *((struct_ptr as usize + CLEAN_ALL_FLAG_OFFSET) as *mut u8) = 0;

    let _ = log_line("[Config] CleanAll completed");
}

// ============================================================
// GameInfo__LoadOptions replacement (0x460AC0)
// ============================================================

/// Rust replacement for GameInfo__LoadOptions.
/// stdcall(game_info: u32)
///
/// Reads game options from the Windows registry and copies various globals
/// into the GameInfo struct at known offsets.
unsafe extern "stdcall" fn hook_load_options(gi_ptr: u32) {
    use openwa_core::wa::registry::read_profile_int;
    use openwa_core::game_info::GameInfo;

    let gi = &mut *(gi_ptr as *mut GameInfo);

    let _ = log_line("[Config] LoadOptions: loading game options from registry");

    // Format speech path: "%s\\user\\speech"
    let base_dir = rb(va::G_BASE_DIR) as *const u8;
    let base_str = std::ffi::CStr::from_ptr(base_dir as *const i8);
    let speech_path = format!("{}\\user\\speech\0", base_str.to_string_lossy());
    gi.speech_path[..speech_path.len()].copy_from_slice(speech_path.as_bytes());

    // Copy 64 bytes from global 0x88DFF3
    core::ptr::copy_nonoverlapping(
        rb(va::G_GAMEINFO_BLOCK_F485) as *const u8,
        gi._config_block_f485.as_mut_ptr(),
        64,
    );

    // Format streams directory and randomize stream indices (global, not GameInfo)
    let streams_dest = rb(va::G_STREAMS_DIR) as *mut u8;
    let streams_path = format!("{}\\streams\0", base_str.to_string_lossy());
    core::ptr::copy_nonoverlapping(
        streams_path.as_ptr(),
        streams_dest,
        streams_path.len(),
    );

    // Randomize stream indices (16 entries, each rand() % 11 + 1)
    let indices = rb(va::G_STREAM_INDICES) as *mut u32;
    let indices_end = rb(va::G_STREAM_INDICES_END) as usize;
    let mut ptr = indices;
    while (ptr as usize) < indices_end {
        extern "cdecl" { fn rand() -> i32; }
        *ptr = (rand() % 11 + 1) as u32;
        ptr = ptr.add(1);
    }

    // Stream volume: 0x10 if flag set, else 0
    let stream_vol_addr = rb(va::G_STREAM_INDICES_END) as *mut u8;
    *stream_vol_addr = if *(rb(va::G_STREAM_FLAG) as *const u32) != 0 { 0x10 } else { 0 };
    *(rb(va::G_STREAM_VOLUME) as *mut u8) = 0x4B;

    // Copy "data\land.dat" string (14 bytes)
    core::ptr::copy_nonoverlapping(
        rb(va::G_LAND_DAT_STRING) as *const u8,
        gi.land_dat_path.as_mut_ptr(),
        14,
    );

    // Copy byte from global
    gi._config_byte_f3a0 = *(rb(va::G_CONFIG_BYTE_F3A0) as *const u8);

    // Registry values from "Options" section
    gi.detail_level = read_profile_int("Options", "DetailLevel", 5) as u8;
    gi._zeroed_f3f0 = 0;

    // Copy 5 DWORDs from globals (indices 0..5 of config block)
    let src = rb(va::G_CONFIG_DWORDS_F3B4) as *const u32;
    for i in 0..5 {
        gi._config_block_f3b4[i] = *src.add(i);
    }

    // Conditional copy: 4 DWORDs if guard == 0
    if *(rb(va::G_CONFIG_GUARD) as *const u32) == 0 {
        let src = rb(va::G_CONFIG_DWORDS_F3F4) as *const u32;
        for i in 0..4 {
            gi._conditional_config_f3f4[i] = *src.add(i);
        }
    }

    // Single DWORDs from globals
    gi._config_dword_dae8 = *(rb(va::G_CONFIG_DWORD_DAE8) as *const u32);

    let src_d4 = rb(va::G_CONFIG_DWORDS_F3D4) as *const u32;
    gi._config_dword_f3d4 = *src_d4;
    gi._config_dword_f3d8 = *src_d4.add(1);

    gi.energy_bar = read_profile_int("Options", "EnergyBar", 1) as u8;

    // 3 DWORDs from globals (overwrite indices 4..7 of config block)
    let src_c4 = rb(va::G_CONFIG_DWORDS_F3C4) as *const u32;
    for i in 0..3 {
        gi._config_block_f3b4[4 + i] = *src_c4.add(i);
    }

    gi.info_transparency = read_profile_int("Options", "InfoTransparency", 0) as u8;
    gi.info_spy = if read_profile_int("Options", "InfoSpy", 1) != 0 { 1 } else { 0 };
    gi.chat_pinned = read_profile_int("Options", "ChatPinned", 0) as u8;
    gi.chat_lines = read_profile_int("Options", "ChatLines", 0);
    gi.pinned_chat_lines = read_profile_int("Options", "PinnedChatLines", 0xFFFFFFFF);
    gi.home_lock = read_profile_int("Options", "HomeLock", 0) as u8;

    // BackgroundDebrisParallax: clamp to i16 range, then << 16
    let mut parallax = read_profile_int("Options", "BackgroundDebrisParallax", 0x50);
    let parallax_i32 = parallax as i32;
    if parallax_i32 < -0x8000 || parallax_i32 > 0x7FFF {
        if parallax_i32 < 0 {
            parallax = (-0x8000i32) as u32;
        } else {
            parallax = 0x7FFF;
        }
    }
    gi.background_debris_parallax = parallax << 16;

    gi.topmost_explosion_onomatopoeia = read_profile_int("Options", "TopmostExplosionOnomatopoeia", 0);
    gi.capture_transparent_pngs = read_profile_int("Options", "CaptureTransparentPNGs", 0);

    // CameraUnlockMouseSpeed: clamp to max 0xB504, then square
    let mut mouse_speed = read_profile_int("Options", "CameraUnlockMouseSpeed", 0x10);
    if mouse_speed > CAMERA_UNLOCK_SPEED_MAX {
        if (mouse_speed as i32) < 0 {
            mouse_speed = 0;
        } else {
            mouse_speed = CAMERA_UNLOCK_SPEED_MAX;
        }
    }
    gi.camera_unlock_mouse_speed = mouse_speed * mouse_speed;

    gi._config_dword_f3e4 = *(rb(va::G_CONFIG_DWORD_F3E4) as *const u32);

    let _ = log_line("[Config] LoadOptions completed (Rust)");
}

// ============================================================
// Options__GetCrashReportURL replacement (0x5A63F0)
// ============================================================

/// Rust replacement for Options__GetCrashReportURL.
/// cdecl() -> *const u8 (pointer to static buffer, or null)
unsafe extern "cdecl" fn hook_get_crash_report_url() -> u32 {
    let buf = rb(va::G_CRASH_REPORT_URL) as *mut u8;
    let buf_slice = core::slice::from_raw_parts_mut(buf, CRASH_REPORT_URL_BUF_SIZE);

    let len = openwa_core::wa::registry::read_profile_string(
        "Options",
        "CrashReportURL",
        buf_slice,
    );

    if len > 0 {
        // Null-terminate
        *buf.add(len) = 0;
        buf as u32
    } else {
        0 // null pointer = not found
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

        let _ = crate::hook::install(
            "GameInfo__LoadOptions",
            va::GAMEINFO_LOAD_OPTIONS,
            hook_load_options as *const (),
        )?;

        let _ = crate::hook::install(
            "Options__GetCrashReportURL",
            va::OPTIONS_GET_CRASH_REPORT_URL,
            hook_get_crash_report_url as *const (),
        )?;
    }

    Ok(())
}
