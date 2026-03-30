//! Per-instance file path isolation for concurrent test execution.
//!
//! Hooks `kernel32!CreateFileA` to redirect temp/scratch files to a per-PID
//! subdirectory, preventing races when multiple WA.exe instances run
//! simultaneously. Only active when `OPENWA_HEADLESS=1` is set.
//!
//! Redirected files:
//!   Game dir:  writetest.txt, mono.tmp, custom.dat
//!   DATA\:     land.dat, landgen.svg, current.thm, playback.thm
//!   ERRORLOG:  ERRORLOG.TXT → OPENWA_ERRORLOG_PATH (if set)

use crate::log_line;

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};
use std::ffi::CStr;
use std::sync::OnceLock;

type HANDLE = *mut c_void;
type DWORD = u32;
type LpSecurityAttributes = *mut c_void;

type CreateFileAFn = unsafe extern "system" fn(
    lpFileName: *const u8,
    dwDesiredAccess: DWORD,
    dwShareMode: DWORD,
    lpSecurityAttributes: LpSecurityAttributes,
    dwCreationDisposition: DWORD,
    dwFlagsAndAttributes: DWORD,
    hTemplateFile: HANDLE,
) -> HANDLE;

static ORIG_CREATE_FILE_A: AtomicU32 = AtomicU32::new(0);

static ERRORLOG_PATH: OnceLock<Option<String>> = OnceLock::new();
static TEMP_DIR: OnceLock<Option<String>> = OnceLock::new();

fn errorlog_redirect() -> Option<&'static str> {
    ERRORLOG_PATH
        .get_or_init(|| std::env::var("OPENWA_ERRORLOG_PATH").ok())
        .as_deref()
}

/// Returns the per-PID temp directory path, if file isolation is active.
pub fn temp_dir_path() -> Option<&'static str> {
    TEMP_DIR.get().and_then(|o| o.as_deref())
}

/// Get (and lazily create) a per-PID temp directory under the game folder.
fn temp_dir() -> Option<&'static str> {
    TEMP_DIR
        .get_or_init(|| {
            // Build path: {game_dir}\.openwa_tmp\{pid}\
            let pid = std::process::id();
            let mut game_dir = std::env::current_dir().ok()?;
            game_dir.push(format!(".openwa_tmp\\{pid}"));
            std::fs::create_dir_all(&game_dir).ok()?;
            // Also create DATA subdirectory
            std::fs::create_dir_all(game_dir.join("DATA")).ok()?;
            Some(game_dir.to_string_lossy().into_owned())
        })
        .as_deref()
}

/// Files in the game root directory that need isolation.
const ROOT_FILES: &[&str] = &["writetest.txt", "mono.tmp"];

/// Files in the DATA subdirectory that need isolation.
const DATA_FILES: &[&str] = &["land.dat", "landgen.svg", "current.thm", "playback.thm"];

/// Check if a path ends with one of our target filenames (case-insensitive).
/// Returns the replacement path if it matches, None otherwise.
fn redirect_path(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();

    // ERRORLOG.TXT → env var path (if set by test runner)
    if let Some(target) = errorlog_redirect() {
        if lower.ends_with("\\errorlog.txt")
            || lower.ends_with("/errorlog.txt")
            || lower == "errorlog.txt"
        {
            return Some(target.to_string());
        }
    }

    let tmp = temp_dir()?;

    // Check root-level files (writetest.txt, mono.tmp, custom.dat)
    for &name in ROOT_FILES {
        if lower.ends_with(&format!("\\{name}")) || lower == name {
            return Some(format!("{tmp}\\{name}"));
        }
    }

    // Check DATA\ files (land.dat, landgen.svg, current.thm, playback.thm)
    for &name in DATA_FILES {
        let pattern_bs = format!("\\{name}");
        let pattern_fs = format!("/{name}");
        if lower.ends_with(&pattern_bs) || lower.ends_with(&pattern_fs) {
            return Some(format!("{tmp}\\DATA\\{name}"));
        }
    }

    None
}

unsafe extern "system" fn hook_create_file_a(
    lp_file_name: *const u8,
    desired_access: DWORD,
    share_mode: DWORD,
    security_attributes: LpSecurityAttributes,
    creation_disposition: DWORD,
    flags_and_attributes: DWORD,
    template_file: HANDLE,
) -> HANDLE {
    let orig: CreateFileAFn = core::mem::transmute(ORIG_CREATE_FILE_A.load(Ordering::Relaxed));

    if !lp_file_name.is_null() {
        if let Ok(path) = CStr::from_ptr(lp_file_name as *const i8).to_str() {
            if let Some(new_path) = redirect_path(path) {
                let cstr: Vec<u8> = new_path.bytes().chain(std::iter::once(0)).collect();
                return orig(
                    cstr.as_ptr(),
                    desired_access,
                    share_mode,
                    security_attributes,
                    creation_disposition,
                    flags_and_attributes,
                    template_file,
                );
            }
        }
    }

    orig(
        lp_file_name,
        desired_access,
        share_mode,
        security_attributes,
        creation_disposition,
        flags_and_attributes,
        template_file,
    )
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_HEADLESS").is_err() {
        return Ok(()); // Only active in headless mode
    }

    unsafe {
        let module =
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(b"kernel32.dll\0".as_ptr());
        if module.is_null() {
            return Err("kernel32.dll not loaded".to_string());
        }

        let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
            module,
            b"CreateFileA\0".as_ptr(),
        );
        let addr = proc.ok_or("CreateFileA not found in kernel32.dll")?;
        let target = addr as *mut c_void;
        let trampoline = minhook::MinHook::create_hook(target, hook_create_file_a as *mut c_void)
            .map_err(|e| format!("MinHook create_hook failed for CreateFileA: {e}"))?;
        minhook::MinHook::queue_enable_hook(target)
            .map_err(|e| format!("MinHook queue_enable_hook failed for CreateFileA: {e}"))?;

        ORIG_CREATE_FILE_A.store(trampoline as u32, Ordering::Relaxed);

        let pid = std::process::id();
        let tmp_msg = temp_dir()
            .map(|d| format!(" → {d}"))
            .unwrap_or_default();
        let errorlog_msg = errorlog_redirect()
            .map(|t| format!(", ERRORLOG.TXT → {t}"))
            .unwrap_or_default();
        let _ = log_line(&format!(
            "[FileIsolation] Hooked CreateFileA (pid={pid}): temp files{tmp_msg}{errorlog_msg}"
        ));
    }

    Ok(())
}

/// Clean up the per-PID temp directory. Called during DLL detach or test cleanup.
pub fn cleanup() {
    if let Some(tmp) = TEMP_DIR.get().and_then(|o| o.as_deref()) {
        let _ = std::fs::remove_dir_all(tmp);
        // Also try to remove parent .openwa_tmp if empty
        if let Some(parent) = std::path::Path::new(tmp).parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}
