#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::missing_transmute_annotations)]

use std::ffi::c_void;

mod debug_server;
pub(crate) mod debug_sync;
mod debug_ui;
#[allow(dead_code)]
mod debug_watchpoint;
pub(crate) mod generated;
pub mod hook;
mod match_launcher;
mod replacements;
mod snapshot;

// ---------------------------------------------------------------------------
// DllMain
// ---------------------------------------------------------------------------

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;

/// Named event prefix shared with the launcher. The full event name is
/// `OpenWA_HooksReady_{pid}` where pid is the WA.exe process ID. Both the
/// launcher (which knows the child PID) and the DLL (via GetCurrentProcessId)
/// independently construct the same name, enabling concurrent instances.
const HOOKS_READY_EVENT_PREFIX: &str = "OpenWA_HooksReady_";

#[unsafe(no_mangle)]
unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        std::thread::spawn(|| {
            if let Err(e) = run() {
                let _ = log_line(&format!("[FATAL] {e}"));
            }
        });
    } else if reason == DLL_PROCESS_DETACH {
        // Write gameplay milestone report before the process exits.
        // This fires on natural exit, safety timeout, and headless mode.
        replacements::write_gameplay_report();
    }
    1 // TRUE
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

pub use openwa_core::log::log_line;

fn clear_log() -> std::io::Result<()> {
    let path = std::env::var_os("OPENWA_LOG_PATH").unwrap_or("OpenWA.log".into());
    std::fs::write(path, "")?;
    Ok(())
}

/// `Write` adapter over an inherited Win32 pipe handle. Used as the secondary
/// log sink: the launcher creates an anonymous pipe and passes the write end
/// into us via `OPENWA_LOG_PIPE`, so log lines surface live on the launcher's
/// stdout. Stdio inheritance from a console parent to a GUI-subsystem child
/// is silently dropped by modern terminal hosts (conpty), so a dedicated pipe
/// is the only reliable forwarding mechanism.
struct PipeSink {
    handle: *mut std::ffi::c_void,
}

unsafe impl Send for PipeSink {}

impl std::io::Write for PipeSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut written: u32 = 0;
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::WriteFile(
                self.handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(written as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn install_log_pipe_sink() {
    let Some(value) = std::env::var_os("OPENWA_LOG_PIPE") else {
        return;
    };
    let Some(value) = value.to_str() else { return };
    let Ok(handle_int) = value.parse::<usize>() else {
        return;
    };
    if handle_int == 0 {
        return;
    }
    openwa_core::log::set_secondary_sink(Box::new(PipeSink {
        handle: handle_int as *mut std::ffi::c_void,
    }));
}

// ---------------------------------------------------------------------------
// Main entry
// ---------------------------------------------------------------------------

fn run() -> Result<(), String> {
    let _ = clear_log();
    install_log_pipe_sink();
    // Install panic hook that writes to our log file. We force-capture a
    // backtrace because most panics here happen inside `extern "C"` hooks,
    // where the runtime then double-panics with "panic in a function that
    // cannot unwind" — without a captured backtrace the original site is
    // lost.
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let _ = log_line(&format!("[PANIC] {info}\n[PANIC] backtrace:\n{bt}"));
    }));

    // Catch native SEH exceptions (access violations, etc.) that the Rust
    // panic hook doesn't see. Logged once per first-chance exception so we
    // know if a hidden C-side fault is killing the process.
    install_native_exception_logger();

    let delta = openwa_game::rebase::init();
    let _ = log_line(&format!(
        "=== OpenWA DLL loaded ===\n  ASLR delta: 0x{delta:08X}"
    ));

    replacements::install_all()?;

    // All hooks were queued during install_all() — now enable them in one
    // batched VirtualProtect pass instead of one syscall per hook.
    unsafe {
        minhook::MinHook::apply_queued()
            .map_err(|e| format!("MinHook apply_queued failed: {e}"))?;
    }

    let _ = log_line("=== All replacements installed ===");

    // Signal the launcher that all hooks are installed and the main thread
    // can be resumed safely. The launcher holds WA.exe suspended until this
    // event fires. If we weren't launched by our launcher (e.g. WormKit
    // module loading), the event won't exist and this is a harmless no-op.
    signal_hooks_ready();

    // Debug UI window (requires "debug-ui" feature + OPENWA_DEBUG_UI=1)
    debug_ui::maybe_spawn();

    // Custom match-launcher window (requires "match-launcher" feature + OPENWA_FRONTEND=1)
    match_launcher::maybe_spawn();

    // Register the watchpoint-arming callback so the egui frontend can
    // trigger it on-demand for the GameInfo-writer RE workflow.
    match_launcher::register_arm_gameinfo_watchpoints();

    // Debug frame sync (breakpoints, suspend/resume)
    debug_sync::init();

    // Debug server (requires OPENWA_DEBUG_SERVER=1)
    debug_server::maybe_start();

    Ok(())
}

/// Install a vectored exception handler that logs first-chance native
/// SEH exceptions (access violations, divide-by-zero in C code, etc.).
/// These don't fire the Rust panic hook because they originate outside
/// Rust's `extern "C-unwind"` boundary. We log and return EXCEPTION_CONTINUE_SEARCH
/// so the OS still gets to handle the fault (terminate the process).
fn install_native_exception_logger() {
    use windows_sys::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_POINTERS,
    };

    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    // Skip C++ exceptions (used by MFC + STL) — they're not crashes.
    const CXX_EXCEPTION_CODE: u32 = 0xE06D7363;
    // Skip our own watchpoint INT3 + single-step traps.
    const STATUS_BREAKPOINT: u32 = 0x80000003;
    const STATUS_SINGLE_STEP: u32 = 0x80000004;

    unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
        unsafe {
            let rec = (*info).ExceptionRecord;
            let code = (*rec).ExceptionCode as u32;
            if code == CXX_EXCEPTION_CODE || code == STATUS_BREAKPOINT || code == STATUS_SINGLE_STEP
            {
                return EXCEPTION_CONTINUE_SEARCH;
            }
            let addr = (*rec).ExceptionAddress as u32;
            // Two read params for access violations: [0] = 0/1 read/write, [1] = addr
            let info0 = if (*rec).NumberParameters > 0 {
                (*rec).ExceptionInformation[0] as u32
            } else {
                0
            };
            let info1 = if (*rec).NumberParameters > 1 {
                (*rec).ExceptionInformation[1] as u32
            } else {
                0
            };
            let bt = std::backtrace::Backtrace::force_capture();
            let _ = log_line(&format!(
                "[NATIVE-EXCEPTION] code=0x{code:08X} addr=0x{addr:08X} info0={info0} info1=0x{info1:08X}\n{bt}"
            ));
            EXCEPTION_CONTINUE_SEARCH
        }
    }

    unsafe {
        AddVectoredExceptionHandler(1, Some(handler));
    }
}

/// Signal the `OpenWA_HooksReady_{pid}` named event so the launcher knows
/// it's safe to resume WA.exe's main thread.
fn signal_hooks_ready() {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetCurrentProcessId, OpenEventA, SetEvent};

    const EVENT_MODIFY_STATE: u32 = 0x0002;

    unsafe {
        let pid = GetCurrentProcessId();
        let event_name = format!("{}{}\0", HOOKS_READY_EVENT_PREFIX, pid);
        let handle = OpenEventA(
            EVENT_MODIFY_STATE,
            0, // bInheritHandle = FALSE
            event_name.as_ptr(),
        );
        if !handle.is_null() {
            SetEvent(handle);
            CloseHandle(handle);
            let _ = log_line(&format!(
                "=== Signalled {}{} ===",
                HOOKS_READY_EVENT_PREFIX, pid
            ));
        }
        // If handle is null, we weren't launched by our launcher — that's fine.
    }
}
