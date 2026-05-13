//! Configuration system operations — Rust replacements for WA.exe theme,
//! registry, and `GameInfo__LoadOptions` hooks.
//!
//! The DLL crate provides only hook entry stubs and `install()`. All file I/O,
//! registry reads, MessageBox-on-error fall-backs, and game-state writes live
//! here.

use std::ffi::CStr;

use windows_sys::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER};

use crate::address::va;
use crate::engine::GameInfo;
use crate::rebase::rb;
use crate::wa::registry::{
    delete_key_recursive as registry_delete, read_profile_int, read_profile_string,
};

const THEME_PATH: &str = "data\\current.thm";
const CLEAN_ALL_FLAG_OFFSET: usize = 0xE0;
const CRASH_REPORT_URL_BUF_SIZE: usize = 0x400;
/// Max CameraUnlockMouseSpeed before squaring — sqrt(2^31) ≈ 46340, prevents overflow.
const CAMERA_UNLOCK_SPEED_MAX: u32 = 0xB504;

unsafe extern "system" {
    fn WriteProfileSectionA(app_name: *const u8, string: *const u8) -> i32;
}

unsafe extern "cdecl" {
    fn rand() -> i32;
}

/// Show an AfxMessageBox-equivalent error dialog.
fn show_error_message(msg: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut buf: Vec<u8> = msg.bytes().collect();
    buf.push(0);
    unsafe {
        MessageBoxA(
            core::ptr::null_mut(),
            buf.as_ptr(),
            core::ptr::null(),
            MB_OK,
        );
    }
}

// ─── Theme__GetFileSize (0x44BA80) ──────────────────────────────────────────

pub fn theme_get_file_size() -> u32 {
    match std::fs::metadata(THEME_PATH) {
        Ok(m) => m.len() as u32,
        Err(_) => 0,
    }
}

// ─── Theme__Load (0x44BB20) ─────────────────────────────────────────────────

pub unsafe fn theme_load(dest: *mut u8) {
    unsafe {
        match std::fs::read(THEME_PATH) {
            Ok(data) => {
                core::ptr::copy_nonoverlapping(data.as_ptr(), dest, data.len());
            }
            Err(_) => {
                show_error_message("ERROR: NO CURRENT.THM FILE FOUND");
            }
        }
    }
}

// ─── Theme__Save (0x44BBC0) ─────────────────────────────────────────────────

pub unsafe fn theme_save(buffer: *const u8, size: u32) {
    unsafe {
        let data = core::slice::from_raw_parts(buffer, size as usize);
        if std::fs::write(THEME_PATH, data).is_err() {
            show_error_message("ERROR: Could Not create CURRENT.THM File");
        }
    }
}

// ─── Registry__DeleteKeyRecursive (0x4E4D10) ────────────────────────────────

pub unsafe fn delete_key_recursive(hkey: HKEY, subkey: *const u8) -> i32 {
    unsafe {
        let c_subkey = CStr::from_ptr(subkey as *const i8);
        let subkey_str = c_subkey.to_string_lossy();
        registry_delete(hkey, &subkey_str) as i32
    }
}

// ─── Registry__CleanAll (0x4C90D0) ──────────────────────────────────────────

pub unsafe fn registry_clean_all(struct_ptr: *mut u8) {
    unsafe {
        let sections = [
            "Software\\Team17SoftwareLTD\\WormsArmageddon\\Data",
            "Software\\Team17SoftwareLTD\\WormsArmageddon\\Options",
            "Software\\Team17SoftwareLTD\\WormsArmageddon\\ExportVideo",
            "Software\\Team17SoftwareLTD\\WormsArmageddon\\VSyncAssist",
        ];

        for section in &sections {
            registry_delete(HKEY_CURRENT_USER, section);
        }

        WriteProfileSectionA(c"NetSettings".as_ptr().cast(), c"".as_ptr().cast());

        *struct_ptr.add(CLEAN_ALL_FLAG_OFFSET) = 0;
    }
}

/// Rust replacement for `GameInfo__InitSession` (0x004608E0).
///
/// Thin wrapper around [`crate::engine::init_session::init_session`] kept
/// here for source-compat with existing callers (`openwa-frontend`,
/// future custom-launcher paths). The original WA function is retained at
/// `va::GAMEINFO_INIT_SESSION` for reference but no longer called.
pub unsafe fn init_session(type_label: Option<&core::ffi::CStr>) {
    unsafe {
        let gi = rb(va::G_GAME_INFO) as *mut GameInfo;
        crate::engine::init_session::init_session(gi, type_label);
    }
}

// ─── GameInfo__LoadOptions (0x460AC0) ───────────────────────────────────────

/// Reads game options from the Windows registry and copies various globals
/// into `GameInfo` at known offsets. `game_info` is the *inner* `G_GAME_INFO`
/// pointer; the LoadOptions MinHook shim ([`crate::engine::init_session`])
/// adjusts WA's `prefix_ptr` arg by `+0x40` before calling.
///
/// (Historical note: pre-2026-05-13 this function had `gi`/`inner_gi`
/// duality because the Rust `GameInfo` struct had prefix-coord field
/// offsets in the upper region. The 2026-05-13 cluster refactor migrated
/// every upper-region field to its inner-coord position, eliminating the
/// duality. See git history for the four-cluster transition.)
pub unsafe fn load_options(game_info: *mut GameInfo) {
    unsafe {
        let inner_gi = &mut *game_info;

        // Format speech path: "%s\\user\\speech" — inner offset 0xF3C4.
        let base_dir = rb(va::G_BASE_DIR) as *const u8;
        let base_str = CStr::from_ptr(base_dir as *const i8);
        let speech_path = format!("{}\\user\\speech\0", base_str.to_string_lossy());
        inner_gi.speech_path[..speech_path.len()].copy_from_slice(speech_path.as_bytes());

        // Copy 64 bytes from global 0x0088DFF3 — inner offset 0xF445.
        core::ptr::copy_nonoverlapping(
            rb(va::G_GAMEINFO_BLOCK_F485) as *const u8,
            inner_gi._config_block_f445.as_mut_ptr(),
            64,
        );

        // Format streams directory and randomize stream indices (global, not GameInfo)
        let streams_dest = rb(va::G_STREAMS_DIR) as *mut u8;
        let streams_path = format!("{}\\streams\0", base_str.to_string_lossy());
        core::ptr::copy_nonoverlapping(streams_path.as_ptr(), streams_dest, streams_path.len());

        // Randomize stream indices (16 entries, each rand() % 11 + 1)
        let indices = rb(va::G_STREAM_INDICES) as *mut u32;
        let indices_end = rb(va::G_STREAM_INDICES_END) as usize;
        let mut ptr = indices;
        while (ptr as usize) < indices_end {
            *ptr = (rand() % 11 + 1) as u32;
            ptr = ptr.add(1);
        }

        // Stream volume: 0x10 if flag set, else 0
        let stream_vol_addr = rb(va::G_STREAM_INDICES_END) as *mut u8;
        *stream_vol_addr = if *(rb(va::G_STREAM_FLAG) as *const u32) != 0 {
            0x10
        } else {
            0
        };
        *(rb(va::G_STREAM_VOLUME) as *mut u8) = 0x4B;

        // Copy "data\land.dat" string (14 bytes) into the first 14 bytes
        // of `landscape_data_path` (inner offset 0xDAAC). Cluster-1 field —
        // accessed via `inner_gi`.
        core::ptr::copy_nonoverlapping(
            rb(va::G_LAND_DAT_STRING) as *const u8,
            inner_gi.landscape_data_path.as_mut_ptr(),
            14,
        );

        // Cluster-2 fields (inner offsets 0xF360..0xF370) — accessed via inner_gi.
        inner_gi._config_byte_f360 = *(rb(va::G_CONFIG_BYTE_F3A0) as *const u8);

        // Registry values from "Options" section
        inner_gi.detail_level = read_profile_int("Options", "DetailLevel", 5) as u8;
        inner_gi._zeroed_f3b0 = 0;

        // General cfg block: 5 dwords from G_CONFIG_DWORDS_F3B4. Written
        // sparsely — indices 0..4 at inner 0xF374..0xF383, then index 4
        // jumps to inner 0xF390. Cluster-3 fields, via inner_gi.
        let src = rb(va::G_CONFIG_DWORDS_F3B4) as *const u32;
        inner_gi.display_flags = *src;
        inner_gi._cfg_dword_f378 = *src.add(1);
        inner_gi._cfg_dword_f37c = *src.add(2);
        inner_gi._cfg_dword_f380 = *src.add(3);
        // (index 4 written further down at inner+0xF390)

        if *(rb(va::G_CONFIG_GUARD) as *const u32) == 0 {
            // Conditional cfg block: 4 dwords. The first two ARE display_width
            // and display_height (inner offsets 0xF3B4, 0xF3B8) per
            // `GameEngine::InitHardware`. Last two are unnamed-but-real fields.
            let src = rb(va::G_CONFIG_DWORDS_F3F4) as *const u32;
            inner_gi.display_width = *src;
            inner_gi.display_height = *src.add(1);
            inner_gi._conditional_config_f3bc = *src.add(2);
            inner_gi._conditional_config_f3c0 = *src.add(3);
        }

        // `sound_volume_percent` lives at inner offset 0xDAA8 — Cluster-1
        // field accessed via `inner_gi`. The DAT_88E390 global is just the
        // option-screen / registry-sourced volume value.
        inner_gi.sound_volume_percent = *(rb(va::G_CONFIG_DWORD_DAE8) as *const i32);

        // Sparse cfg block index 4 lands at inner+0xF390, then F3D4/F3D8 follow.
        let cfg_block_4 = *(rb(va::G_CONFIG_DWORDS_F3B4) as *const u32).add(4);
        inner_gi._cfg_dword_f390 = cfg_block_4;
        let src_d4 = rb(va::G_CONFIG_DWORDS_F3D4) as *const u32;
        inner_gi._cfg_dword_f394 = *src_d4;
        inner_gi._field_f398 = *src_d4.add(1) as i32;

        inner_gi.energy_bar = read_profile_int("Options", "EnergyBar", 1) as u8;

        // Second cfg block (G_CONFIG_DWORDS_F3C4, 3 dwords) writes to the
        // gap between indices 3 and 4 of the sparse general block at
        // inner+0xF384, 0xF388, 0xF38C — i.e. `_field_f384`, `_cfg_dword_f388`,
        // `sound_attenuation`.
        let src_c4 = rb(va::G_CONFIG_DWORDS_F3C4) as *const u32;
        inner_gi._field_f384 = *src_c4;
        inner_gi._cfg_dword_f388 = *src_c4.add(1);
        inner_gi.sound_attenuation = *src_c4.add(2) as i32;

        inner_gi.info_transparency = read_profile_int("Options", "InfoTransparency", 0) as u8;
        inner_gi.info_spy = if read_profile_int("Options", "InfoSpy", 1) != 0 {
            1
        } else {
            0
        };
        inner_gi.option_byte_f365 = read_profile_int("Options", "ChatPinned", 0) as u8;
        inner_gi.option_dword_f368 = read_profile_int("Options", "ChatLines", 0);
        inner_gi.option_dword_f36c = read_profile_int("Options", "PinnedChatLines", 0xFFFFFFFF);
        inner_gi.home_lock = read_profile_int("Options", "HomeLock", 0) as u8;

        // BackgroundDebrisParallax: clamp to i16 range, then << 16
        let mut parallax = read_profile_int("Options", "BackgroundDebrisParallax", 0x50);
        let parallax_i32 = parallax as i32;
        if !(-0x8000..=0x7FFF).contains(&parallax_i32) {
            if parallax_i32 < 0 {
                parallax = (-0x8000i32) as u32;
            } else {
                parallax = 0x7FFF;
            }
        }
        inner_gi.background_debris_parallax = parallax << 16;

        inner_gi.topmost_explosion_onomatopoeia =
            read_profile_int("Options", "TopmostExplosionOnomatopoeia", 0);
        // capture_transparent_pngs lives at inner offset 0xF39C (cluster 2).
        inner_gi.capture_transparent_pngs =
            read_profile_int("Options", "CaptureTransparentPNGs", 0);

        // CameraUnlockMouseSpeed: clamp to max 0xB504, then square
        let mut mouse_speed = read_profile_int("Options", "CameraUnlockMouseSpeed", 0x10);
        if mouse_speed > CAMERA_UNLOCK_SPEED_MAX {
            if (mouse_speed as i32) < 0 {
                mouse_speed = 0;
            } else {
                mouse_speed = CAMERA_UNLOCK_SPEED_MAX;
            }
        }
        inner_gi.camera_unlock_mouse_speed = mouse_speed * mouse_speed;

        inner_gi._config_dword_f3a4 = *(rb(va::G_CONFIG_DWORD_F3E4) as *const u32);
    }
}

// ─── Options__GetCrashReportURL (0x5A63F0) ──────────────────────────────────

/// Returns pointer to a static buffer, or null if registry value is missing.
pub unsafe fn get_crash_report_url() -> *mut u8 {
    unsafe {
        let buf = rb(va::G_CRASH_REPORT_URL) as *mut u8;
        let buf_slice = core::slice::from_raw_parts_mut(buf, CRASH_REPORT_URL_BUF_SIZE);

        let len = read_profile_string("Options", "CrashReportURL", buf_slice);

        if len > 0 {
            *buf.add(len) = 0;
            buf
        } else {
            core::ptr::null_mut()
        }
    }
}
