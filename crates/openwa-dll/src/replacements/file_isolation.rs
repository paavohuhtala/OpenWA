//! Per-instance file path isolation for concurrent test execution.
//!
//! Hooks `kernel32!CreateFileA`, `FindFirstFileA`, and `DeleteFileA` to redirect
//! temp/scratch files to a per-PID subdirectory, preventing races when multiple
//! WA.exe instances run simultaneously. Only active when `OPENWA_HEADLESS=1`.
//!
//! Redirected files:
//!   Game dir:  writetest.txt, mono.tmp, custom.dat, thm.prv
//!   DATA\:     land.dat, landgen.svg, current.thm, playback.thm
//!   ERRORLOG:  ERRORLOG.TXT → OPENWA_ERRORLOG_PATH (if set)

use crate::log_line;

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};
use std::ffi::CStr;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HANDLE;

type LpSecurityAttributes = *mut c_void;

type CreateFileAFn = unsafe extern "system" fn(
    lpFileName: *const u8,
    dwDesiredAccess: u32,
    dwShareMode: u32,
    lpSecurityAttributes: LpSecurityAttributes,
    dwCreationDisposition: u32,
    dwFlagsAndAttributes: u32,
    hTemplateFile: HANDLE,
) -> HANDLE;

type FindFirstFileAFn =
    unsafe extern "system" fn(lpFileName: *const u8, lpFindFileData: *mut u8) -> HANDLE;

type DeleteFileAFn = unsafe extern "system" fn(lpFileName: *const u8) -> i32;

static ORIG_CREATE_FILE_A: AtomicU32 = AtomicU32::new(0);
static ORIG_FIND_FIRST_FILE_A: AtomicU32 = AtomicU32::new(0);
static ORIG_DELETE_FILE_A: AtomicU32 = AtomicU32::new(0);

static ERRORLOG_PATH: OnceLock<Option<String>> = OnceLock::new();
static TEMP_DIR: OnceLock<Option<String>> = OnceLock::new();

fn errorlog_redirect() -> Option<&'static str> {
    ERRORLOG_PATH
        .get_or_init(|| std::env::var("OPENWA_ERRORLOG_PATH").ok())
        .as_deref()
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
/// `thm.prv` is a temporary file created/read/deleted by MAP_VIEW_LOAD during
/// terrain processing — concurrent instances fight over it without isolation.
const ROOT_FILES: &[&str] = &["writetest.txt", "mono.tmp", "custom.dat", "thm.prv"];

/// Files in the DATA subdirectory that need isolation.
const DATA_FILES: &[&str] = &["land.dat", "landgen.svg", "current.thm", "playback.thm"];

/// Check if a path ends with one of our target filenames (case-insensitive).
/// Returns the replacement path if it matches, None otherwise.
fn redirect_path(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();

    // ERRORLOG.TXT → env var path (if set by test runner)
    if let Some(target) = errorlog_redirect()
        && (lower.ends_with("\\errorlog.txt")
            || lower.ends_with("/errorlog.txt")
            || lower == "errorlog.txt")
    {
        return Some(target.to_string());
    }

    let tmp = temp_dir()?;

    // Check root-level files
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

// ─── CreateFileA hook ──────────────────────────────────────────────────────

/// WA.exe opens data files with `dwShareMode=0` (exclusive access), which blocks
/// concurrent instances from reading the same files. In headless mode, force
/// `FILE_SHARE_READ` on all file opens so multiple test instances can coexist.
unsafe extern "system" fn hook_create_file_a(
    lp_file_name: *const u8,
    desired_access: u32,
    share_mode: u32,
    security_attributes: LpSecurityAttributes,
    creation_disposition: u32,
    flags_and_attributes: u32,
    template_file: HANDLE,
) -> HANDLE {
    unsafe {
        let orig: CreateFileAFn = core::mem::transmute(ORIG_CREATE_FILE_A.load(Ordering::Relaxed));

        // Force FILE_SHARE_READ on all opens to prevent exclusive locking between
        // concurrent WA.exe instances. WA opens .img, .wav, .bmp etc. with share=0,
        // which causes "File Error" failures under high concurrency.
        const FILE_SHARE_READ: u32 = 0x00000001;
        let share_mode = share_mode | FILE_SHARE_READ;

        if !lp_file_name.is_null()
            && let Ok(path) = CStr::from_ptr(lp_file_name as *const i8).to_str()
            && let Some(new_path) = redirect_path(path)
        {
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
}

// ─── FindFirstFileA hook ───────────────────────────────────────────────────

/// Redirect FindFirstFileA for isolated files. MAP_VIEW_LOAD's file reader
/// (FUN_004dfa70) uses FindFirstFileA to get the file size before _fopen.
/// Without this hook, it looks in the game directory instead of the per-PID
/// temp directory where CreateFileA wrote the file.
unsafe extern "system" fn hook_find_first_file_a(
    lp_file_name: *const u8,
    lp_find_file_data: *mut u8,
) -> HANDLE {
    unsafe {
        let orig: FindFirstFileAFn =
            core::mem::transmute(ORIG_FIND_FIRST_FILE_A.load(Ordering::Relaxed));

        if !lp_file_name.is_null()
            && let Ok(path) = CStr::from_ptr(lp_file_name as *const i8).to_str()
            && let Some(new_path) = redirect_path(path)
        {
            let cstr: Vec<u8> = new_path.bytes().chain(std::iter::once(0)).collect();
            return orig(cstr.as_ptr(), lp_find_file_data);
        }

        orig(lp_file_name, lp_find_file_data)
    }
}

// ─── DeleteFileA hook ──────────────────────────────────────────────────────

/// Redirect DeleteFileA for isolated files. MAP_VIEW_LOAD deletes the temporary
/// `thm.prv` file after reading it. Without this hook, the delete targets the
/// game directory while the file is in the per-PID temp directory.
unsafe extern "system" fn hook_delete_file_a(lp_file_name: *const u8) -> i32 {
    unsafe {
        let orig: DeleteFileAFn = core::mem::transmute(ORIG_DELETE_FILE_A.load(Ordering::Relaxed));

        if !lp_file_name.is_null()
            && let Ok(path) = CStr::from_ptr(lp_file_name as *const i8).to_str()
            && let Some(new_path) = redirect_path(path)
        {
            let cstr: Vec<u8> = new_path.bytes().chain(std::iter::once(0)).collect();
            return orig(cstr.as_ptr());
        }

        orig(lp_file_name)
    }
}

// ─── File-exists check hook ────────────────────────────────────────────────

/// WA's file-existence check (0x4DFA30) uses `_findfirst` which does a directory
/// enumeration. Under high concurrency, NTFS directory contention causes transient
/// failures. Replace with a simple "always exists" in headless mode — the checked
/// files (steam.dat, graphics\Font.bmp) are guaranteed present in the game dir.
unsafe extern "fastcall" fn hook_file_exists_check(_filename: *const u8) -> u32 {
    1 // always report file as existing
}

// ─── Installation ──────────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    let is_test =
        std::env::var("OPENWA_HEADLESS").is_ok() || std::env::var("OPENWA_REPLAY_TEST").is_ok();

    if !is_test {
        return Ok(()); // File isolation only active during tests
    }

    unsafe {
        let k32 = windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(
            c"kernel32.dll".as_ptr().cast(),
        );
        if k32.is_null() {
            return Err("kernel32.dll not loaded".to_string());
        }

        // Hook CreateFileA — path redirection + FILE_SHARE_READ forcing
        hook_kernel32_fn(
            k32,
            c"CreateFileA",
            hook_create_file_a as *mut c_void,
            &ORIG_CREATE_FILE_A,
        )?;

        // Hook FindFirstFileA — path redirection for file size lookups
        hook_kernel32_fn(
            k32,
            c"FindFirstFileA",
            hook_find_first_file_a as *mut c_void,
            &ORIG_FIND_FIRST_FILE_A,
        )?;

        // Hook DeleteFileA — path redirection for temp file cleanup
        hook_kernel32_fn(
            k32,
            c"DeleteFileA",
            hook_delete_file_a as *mut c_void,
            &ORIG_DELETE_FILE_A,
        )?;

        // Hook file-exists check to avoid _findfirst contention under concurrency
        crate::hook::install(
            "FileExistsCheck",
            openwa_game::address::va::FILE_EXISTS_CHECK,
            hook_file_exists_check as *const (),
        )?;

        let pid = std::process::id();
        let tmp_msg = temp_dir().map(|d| format!(" → {d}")).unwrap_or_default();
        let errorlog_msg = errorlog_redirect()
            .map(|t| format!(", ERRORLOG.TXT → {t}"))
            .unwrap_or_default();
        let _ = log_line(&format!(
            "[FileIsolation] Hooked CreateFileA+FindFirstFileA+DeleteFileA (pid={pid}): temp files{tmp_msg}{errorlog_msg}"
        ));
    }

    Ok(())
}

/// Helper: hook a kernel32 function via MinHook.
unsafe fn hook_kernel32_fn(
    module: *mut c_void,
    name: &core::ffi::CStr,
    hook_fn: *mut c_void,
    orig_store: &AtomicU32,
) -> Result<(), String> {
    unsafe {
        let fn_name = name.to_str().unwrap_or("?");
        let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
            module as _,
            name.as_ptr().cast(),
        );
        let addr = proc.ok_or(format!("{fn_name} not found in kernel32.dll"))?;
        let target = addr as *mut c_void;
        let trampoline = minhook::MinHook::create_hook(target, hook_fn)
            .map_err(|e| format!("MinHook create_hook failed for {fn_name}: {e}"))?;
        minhook::MinHook::queue_enable_hook(target)
            .map_err(|e| format!("MinHook queue_enable_hook failed for {fn_name}: {e}"))?;
        orig_store.store(trampoline as u32, Ordering::Relaxed);
        Ok(())
    }
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
