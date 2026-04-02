mod inject;

use std::ffi::CString;
use std::mem;
use std::path::{Path, PathBuf};
use std::ptr;

// Declare Win32 types and functions directly to avoid windows-sys feature-flag issues.
#[allow(non_camel_case_types)]
type HANDLE = *mut core::ffi::c_void;
#[allow(non_camel_case_types)]
type HMODULE = *mut core::ffi::c_void;
#[allow(non_camel_case_types)]
type DWORD = u32;
#[allow(non_camel_case_types)]
type BOOL = i32;
#[allow(non_camel_case_types)]
type LPVOID = *mut core::ffi::c_void;
#[allow(non_camel_case_types)]
type LPDWORD = *mut u32;

#[repr(C)]
#[allow(non_snake_case)]
struct STARTUPINFOA {
    cb: DWORD,
    lpReserved: *mut u8,
    lpDesktop: *mut u8,
    lpTitle: *mut u8,
    dwX: DWORD,
    dwY: DWORD,
    dwXSize: DWORD,
    dwYSize: DWORD,
    dwXCountChars: DWORD,
    dwYCountChars: DWORD,
    dwFillAttribute: DWORD,
    dwFlags: DWORD,
    wShowWindow: u16,
    cbReserved2: u16,
    lpReserved2: *mut u8,
    hStdInput: HANDLE,
    hStdOutput: HANDLE,
    hStdError: HANDLE,
}

#[repr(C)]
#[allow(non_snake_case)]
struct PROCESS_INFORMATION {
    hProcess: HANDLE,
    hThread: HANDLE,
    dwProcessId: DWORD,
    dwThreadId: DWORD,
}

const CREATE_SUSPENDED: DWORD = 0x0000_0004;
const CREATE_NO_WINDOW: DWORD = 0x0800_0000;
const STARTF_USESHOWWINDOW: DWORD = 0x0000_0001;
const SW_HIDE: u16 = 0;
const SW_SHOWMINIMIZED: u16 = 2;
const INFINITE: DWORD = 0xFFFF_FFFF;

#[link(name = "kernel32")]
extern "system" {
    fn CreateProcessA(
        lpApplicationName: *const u8,
        lpCommandLine: *mut u8,
        lpProcessAttributes: LPVOID,
        lpThreadAttributes: LPVOID,
        bInheritHandles: BOOL,
        dwCreationFlags: DWORD,
        lpEnvironment: LPVOID,
        lpCurrentDirectory: *const u8,
        lpStartupInfo: *const STARTUPINFOA,
        lpProcessInformation: *mut PROCESS_INFORMATION,
    ) -> BOOL;
    fn ResumeThread(hThread: HANDLE) -> DWORD;
    fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
    fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: LPDWORD) -> BOOL;
    fn TerminateProcess(hProcess: HANDLE, uExitCode: u32) -> BOOL;
    fn CloseHandle(hObject: HANDLE) -> BOOL;
    fn GetModuleFileNameA(hModule: HMODULE, lpFilename: *mut u8, nSize: DWORD) -> DWORD;
    fn CreateEventA(
        lpEventAttributes: LPVOID,
        bManualReset: BOOL,
        bInitialState: BOOL,
        lpName: *const u8,
    ) -> HANDLE;
}

fn main() {
    if let Err(e) = run() {
        eprintln!("openwa-launcher: error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();

    // Parse --minimized flag; collect remaining args as WA.exe args.
    let mut minimized = false;
    let mut wa_path_arg: Option<String> = None;
    let mut wa_args: Vec<String> = Vec::new();

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--minimized" => minimized = true,
            s if wa_path_arg.is_none() && !s.starts_with("--") => {
                wa_path_arg = Some(s.to_string());
            }
            s => wa_args.push(s.to_string()),
        }
    }

    // Locate WA.exe: explicit arg > env var / registry (via openwa-config).
    let wa_exe = if let Some(p) = wa_path_arg {
        PathBuf::from(p)
    } else {
        openwa_config::find_wa_dir()
            .map(|d| d.join("WA.exe"))
            .ok_or("WA.exe not found. Set OPENWA_WA_PATH or pass the path as an argument.")?
    };

    if !wa_exe.exists() {
        return Err(format!("WA.exe not found at: {}", wa_exe.display()));
    }

    // Locate the DLL: same directory as the launcher executable.
    let dll_path = launcher_dir()?.join("openwa_wormkit.dll");
    if !dll_path.exists() {
        return Err(format!(
            "openwa_wormkit.dll not found at: {}\nBuild with: cargo build --release -p openwa-wormkit",
            dll_path.display()
        ));
    }

    // WA.exe must run with its own folder as the working directory (relative paths for data/logs).
    let wa_dir = wa_exe
        .parent()
        .ok_or("WA.exe path has no parent directory")?;

    // In headless mode, suppress WA.exe's crash dialog. The crash handler
    // (SEH at 0x5A5A20) checks this flag before deciding to relaunch with
    // /handlecrash — if set, it silently writes ERRORLOG.TXT and returns.
    if std::env::var("OPENWA_HEADLESS").is_ok() {
        wa_args.push("/silentcrash".to_string());
    }

    let cmdline = build_cmdline(&wa_exe, &wa_args);

    println!(
        "openwa-launcher: {} + {}",
        wa_exe.display(),
        dll_path.display()
    );

    unsafe { launch(&wa_exe, &cmdline, wa_dir, &dll_path, minimized) }
}

unsafe fn launch(
    wa_exe: &Path,
    cmdline: &str,
    working_dir: &Path,
    dll_path: &Path,
    minimized: bool,
) -> Result<(), String> {
    let exe_cstr = path_to_cstring(wa_exe)?;
    let mut cmdline_buf: Vec<u8> = cmdline.bytes().chain(std::iter::once(0u8)).collect();
    let wd_cstr = path_to_cstring(working_dir)?;

    let headless = std::env::var("OPENWA_HEADLESS").is_ok();

    let mut si: STARTUPINFOA = mem::zeroed();
    si.cb = mem::size_of::<STARTUPINFOA>() as DWORD;
    if headless {
        si.dwFlags |= STARTF_USESHOWWINDOW;
        si.wShowWindow = SW_HIDE;
    } else if minimized {
        si.dwFlags |= STARTF_USESHOWWINDOW;
        si.wShowWindow = SW_SHOWMINIMIZED;
    }

    let mut pi: PROCESS_INFORMATION = mem::zeroed();

    let ok = CreateProcessA(
        exe_cstr.as_ptr().cast(),
        cmdline_buf.as_mut_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
        0, // bInheritHandles = FALSE
        CREATE_SUSPENDED | if headless { CREATE_NO_WINDOW } else { 0 },
        ptr::null_mut(), // inherit environment
        wd_cstr.as_ptr().cast(),
        &si,
        &mut pi,
    );

    if ok == 0 {
        return Err("CreateProcessA failed — is WA.exe path correct?".to_string());
    }

    let dll_str = dll_path.to_str().ok_or("DLL path is not valid UTF-8")?;

    // Create a named event that the DLL will signal after all hooks are
    // installed. This ensures the main thread doesn't run any WA code
    // before our hooks are in place.
    //
    // Use a per-instance event name based on the child PID to allow
    // concurrent launcher instances (e.g., parallel test runner).
    let event_name_str = format!("OpenWA_HooksReady_{}\0", pi.dwProcessId);
    let event_name = event_name_str.as_bytes();
    // Also set env var so the DLL (in the child process) knows the event name.
    // The child inherits our env, but it was created before we set this var.
    // Instead, we write the event name into a small shared memory region:
    // we use SetEnvironmentVariableA in the child's context — but that's not
    // possible for a suspended process. So we rely on the DLL reading the
    // event name from a fixed pattern: OpenWA_HooksReady_{its_own_pid}.
    // The DLL can call GetCurrentProcessId() to reconstruct the same name.
    let event = CreateEventA(ptr::null_mut(), 1, 0, event_name.as_ptr());
    // event may be null if CreateEventA fails — we'll fall through gracefully.

    if let Err(e) = inject::inject_dll(pi.hProcess, dll_str) {
        if !event.is_null() {
            CloseHandle(event);
        }
        TerminateProcess(pi.hProcess, 1);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        return Err(format!("DLL injection failed: {e}"));
    }

    // Wait for the DLL to signal that all hooks are installed.
    // Use a generous timeout — under high concurrency (e.g. 16 parallel
    // test instances), DLL loading + hook installation can exceed 10s due
    // to I/O contention. 120s matches the test runner's own timeout.
    if !event.is_null() {
        let wait_result = WaitForSingleObject(event, 120_000);
        if wait_result != 0 {
            eprintln!("openwa-launcher: warning: hooks-ready event timed out ({wait_result})");
        }
        CloseHandle(event);
    }

    // All hooks installed — let WA.exe run.
    ResumeThread(pi.hThread);
    CloseHandle(pi.hThread);

    // Wait for WA.exe to exit, then propagate its exit code.
    WaitForSingleObject(pi.hProcess, INFINITE);
    let mut exit_code: DWORD = 0;
    GetExitCodeProcess(pi.hProcess, &mut exit_code);
    CloseHandle(pi.hProcess);

    std::process::exit(exit_code as i32);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn launcher_dir() -> Result<PathBuf, String> {
    let mut buf = vec![0u8; 32768];
    let len = unsafe { GetModuleFileNameA(ptr::null_mut(), buf.as_mut_ptr(), buf.len() as DWORD) };
    if len == 0 {
        return Err("GetModuleFileNameA failed".to_string());
    }
    let path = PathBuf::from(
        std::str::from_utf8(&buf[..len as usize])
            .map_err(|e| format!("launcher path not UTF-8: {e}"))?,
    );
    path.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "launcher path has no parent".to_string())
}

fn build_cmdline(exe: &Path, extra: &[String]) -> String {
    let mut s = format!("\"{}\"", exe.display());
    for arg in extra {
        s.push(' ');
        if arg.contains(' ') {
            s.push('"');
            s.push_str(arg);
            s.push('"');
        } else {
            s.push_str(arg);
        }
    }
    s
}

fn path_to_cstring(p: &Path) -> Result<CString, String> {
    CString::new(
        p.to_str()
            .ok_or_else(|| format!("path not valid UTF-8: {}", p.display()))?,
    )
    .map_err(|e| format!("path contains nul byte: {e}"))
}

