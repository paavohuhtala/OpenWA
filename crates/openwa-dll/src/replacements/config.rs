//! Hook wiring for theme, registry, and `GameInfo__LoadOptions` functions.
//!
//! Thin shim — all logic lives in `openwa_game::engine::config_load`.
//!
//! Hooks:
//! - Theme__GetFileSize (0x44BA80)
//! - Theme__Load (0x44BB20)
//! - Theme__Save (0x44BBC0)
//! - Registry__DeleteKeyRecursive (0x4E4D10)
//! - Registry__CleanAll (0x4C90D0)
//! - GameInfo__LoadOptions (0x460AC0)
//! - Options__GetCrashReportURL (0x5A63F0)

use openwa_game::address::va;
use openwa_game::engine::GameInfo;
use openwa_game::engine::config_load;

use crate::hook;

unsafe extern "cdecl" fn hook_theme_get_file_size() -> u32 {
    config_load::theme_get_file_size()
}

unsafe extern "stdcall" fn hook_theme_load(dest: *mut u8) {
    unsafe { config_load::theme_load(dest) }
}

unsafe extern "stdcall" fn hook_theme_save(buffer: *const u8, size: u32) {
    unsafe { config_load::theme_save(buffer, size) }
}

unsafe extern "stdcall" fn hook_delete_key_recursive(hkey: usize, subkey: *const u8) -> i32 {
    unsafe { config_load::delete_key_recursive(hkey as _, subkey) }
}

unsafe extern "stdcall" fn hook_registry_clean_all(struct_ptr: *mut u8) {
    unsafe { config_load::registry_clean_all(struct_ptr) }
}

/// WA calls `LoadOptions(prefix_ptr)` where `prefix_ptr = G_GAME_INFO - 0x40`.
/// Our `config_load::load_options` takes the *inner* `G_GAME_INFO` pointer
/// (post-2026-05-13 cluster refactor) — adjust before forwarding.
unsafe extern "stdcall" fn hook_load_options(prefix_ptr: *mut u8) {
    unsafe {
        let game_info = prefix_ptr.add(0x40) as *mut GameInfo;
        config_load::load_options(game_info);
    }
}

unsafe extern "cdecl" fn hook_get_crash_report_url() -> *mut u8 {
    unsafe { config_load::get_crash_report_url() }
}

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "Theme__GetFileSize",
            va::THEME_GET_FILE_SIZE,
            hook_theme_get_file_size as *const (),
        )?;
        hook::install("Theme__Load", va::THEME_LOAD, hook_theme_load as *const ())?;
        hook::install("Theme__Save", va::THEME_SAVE, hook_theme_save as *const ())?;
        hook::install(
            "Registry__DeleteKeyRecursive",
            va::REGISTRY_DELETE_KEY_RECURSIVE,
            hook_delete_key_recursive as *const (),
        )?;
        hook::install(
            "Registry__CleanAll",
            va::REGISTRY_CLEAN_ALL,
            hook_registry_clean_all as *const (),
        )?;
        hook::install(
            "GameInfo__LoadOptions",
            va::GAMEINFO_LOAD_OPTIONS,
            hook_load_options as *const (),
        )?;
        hook::install(
            "Options__GetCrashReportURL",
            va::OPTIONS_GET_CRASH_REPORT_URL,
            hook_get_crash_report_url as *const (),
        )?;
    }
    Ok(())
}
