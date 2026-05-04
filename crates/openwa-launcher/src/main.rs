mod inject;

use std::ffi::CString;
use std::mem;
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HMODULE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectA, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameA;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateEventA, CreateProcessA, GetExitCodeProcess, INFINITE,
    PROCESS_INFORMATION, ResumeThread, STARTF_USESHOWWINDOW, STARTUPINFOA, TerminateProcess,
    WaitForSingleObject,
};

const SW_HIDE: u16 = 0;
const SW_SHOWMINIMIZED: u16 = 2;

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
    let dll_path = launcher_dir()?.join("openwa.dll");
    if !dll_path.exists() {
        return Err(format!(
            "openwa.dll not found at: {}\nBuild with: cargo build --release -p openwa-dll",
            dll_path.display()
        ));
    }

    // WA.exe must run with its own folder as the working directory (relative paths for data/logs).
    let wa_dir = wa_exe
        .parent()
        .ok_or("WA.exe path has no parent directory")?;

    // In headless and replay-test modes, suppress WA.exe's crash dialog.
    // The crash handler (SEH at 0x5A5A20) checks this flag before deciding
    // to relaunch with /handlecrash — if set, it silently writes ERRORLOG.TXT
    // and returns instead of spawning a new WA.exe for the error dialog.
    if std::env::var("OPENWA_HEADLESS").is_ok() || std::env::var("OPENWA_REPLAY_TEST").is_ok() {
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
    unsafe {
        let exe_cstr = path_to_cstring(wa_exe)?;
        let mut cmdline_buf: Vec<u8> = cmdline.bytes().chain(std::iter::once(0u8)).collect();
        let wd_cstr = path_to_cstring(working_dir)?;

        let headless = std::env::var("OPENWA_HEADLESS").is_ok();

        let mut si: STARTUPINFOA = mem::zeroed();
        si.cb = mem::size_of::<STARTUPINFOA>() as u32;
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
            ptr::null(),
            ptr::null(),
            0, // bInheritHandles = FALSE
            CREATE_SUSPENDED | if headless { CREATE_NO_WINDOW } else { 0 },
            ptr::null(), // inherit environment
            wd_cstr.as_ptr().cast(),
            &si,
            &mut pi,
        );

        if ok == 0 {
            return Err("CreateProcessA failed — is WA.exe path correct?".to_string());
        }

        // Tie WA.exe's lifetime to ours via a job object with KILL_ON_JOB_CLOSE.
        // When the launcher exits (cleanly, crashes, or is killed), the kernel
        // closes our last handle to the job and terminates everything in it.
        // The job handle is intentionally leaked — it must outlive WA.exe.
        let job = create_kill_on_close_job();
        if !job.is_null() && AssignProcessToJobObject(job, pi.hProcess) == 0 {
            eprintln!(
                "openwa-launcher: warning: AssignProcessToJobObject failed (err {}); WA.exe will not be auto-killed on launcher exit",
                windows_sys::Win32::Foundation::GetLastError()
            );
            CloseHandle(job);
        }
        // Otherwise keep the handle open for the rest of the process lifetime.

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
        let event = CreateEventA(ptr::null(), 1, 0, event_name.as_ptr());
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
        let mut exit_code: u32 = 0;
        GetExitCodeProcess(pi.hProcess, &mut exit_code);
        CloseHandle(pi.hProcess);

        std::process::exit(exit_code as i32);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create an unnamed job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
/// Returns a null handle on failure; caller should treat that as "no auto-kill".
unsafe fn create_kill_on_close_job() -> windows_sys::Win32::Foundation::HANDLE {
    unsafe {
        let job = CreateJobObjectA(ptr::null(), ptr::null());
        if job.is_null() {
            return ptr::null_mut();
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            CloseHandle(job);
            return ptr::null_mut();
        }
        job
    }
}

fn launcher_dir() -> Result<PathBuf, String> {
    let mut buf = vec![0u8; 32768];
    let len = unsafe { GetModuleFileNameA(HMODULE::default(), buf.as_mut_ptr(), buf.len() as u32) };
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
