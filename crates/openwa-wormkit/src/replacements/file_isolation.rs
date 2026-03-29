//! Per-instance file path isolation for concurrent test execution.
//!
//! Hooks `kernel32!CreateFileA` to redirect landscape generation temp files
//! to per-PID paths, preventing races when multiple WA.exe instances run
//! simultaneously. Only active when `OPENWA_HEADLESS=1` is set.
//!
//! Redirected files (in game directory):
//!   mono.tmp        → mono_{pid}.tmp
//!   DATA\land.dat   → DATA\land_{pid}.dat
//!   DATA\landgen.svg → DATA\landgen_{pid}.svg

use crate::log_line;

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};
use std::ffi::CStr;

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

/// Check if a path ends with one of our target filenames (case-insensitive).
/// Returns the replacement path if it matches, None otherwise.
fn redirect_path(path: &str) -> Option<String> {
    let pid = std::process::id();
    let lower = path.to_ascii_lowercase();

    if lower.ends_with("\\mono.tmp") || lower == "mono.tmp" {
        let prefix = &path[..path.len() - "mono.tmp".len()];
        Some(format!("{prefix}mono_{pid}.tmp"))
    } else if lower.ends_with("\\land.dat") || lower.ends_with("/land.dat") {
        let prefix = &path[..path.len() - "land.dat".len()];
        Some(format!("{prefix}land_{pid}.dat"))
    } else if lower.ends_with("\\landgen.svg") || lower.ends_with("/landgen.svg") {
        let prefix = &path[..path.len() - "landgen.svg".len()];
        Some(format!("{prefix}landgen_{pid}.svg"))
    } else {
        None
    }
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
        minhook::MinHook::enable_hook(target)
            .map_err(|e| format!("MinHook enable_hook failed for CreateFileA: {e}"))?;

        ORIG_CREATE_FILE_A.store(trampoline as u32, Ordering::Relaxed);

        let pid = std::process::id();
        let _ = log_line(&format!(
            "[FileIsolation] Hooked CreateFileA (pid={pid}): mono.tmp, land.dat, landgen.svg → per-PID paths"
        ));
    }

    Ok(())
}
