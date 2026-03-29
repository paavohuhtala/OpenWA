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
pub mod hook;
mod replacements;
mod snapshot;
mod validation;

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

#[no_mangle]
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

// ---------------------------------------------------------------------------
// Main entry
// ---------------------------------------------------------------------------

fn run() -> Result<(), String> {
    let _ = clear_log();
    let validation_path =
        std::env::var_os("OPENWA_VALIDATION_LOG_PATH").unwrap_or("OpenWA_validation.log".into());
    let _ = std::fs::write(validation_path, "");

    // Install panic hook that writes to our log file
    std::panic::set_hook(Box::new(|info| {
        let _ = log_line(&format!("[PANIC] {info}"));
    }));

    let delta = openwa_core::rebase::init();
    let _ = log_line(&format!(
        "=== OpenWA WormKit DLL loaded ===\n  ASLR delta: 0x{delta:08X}"
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

    // Run validation if OPENWA_VALIDATE=1
    if std::env::var("OPENWA_VALIDATE").is_ok() {
        let _ = log_line("=== Validation enabled (OPENWA_VALIDATE) ===");
        if let Err(e) = validation::run() {
            let _ = log_line(&format!("[ERROR] Validation failed: {e}"));
        }
    }

    // Debug hotkeys (F9/F10) are always available, even without OPENWA_VALIDATE
    validation::start_hotkeys();

    // Debug UI window (requires "debug-ui" feature + OPENWA_DEBUG_UI=1)
    debug_ui::maybe_spawn();

    // Debug frame sync (breakpoints, suspend/resume)
    debug_sync::init();

    // Debug server (requires OPENWA_DEBUG_SERVER=1)
    debug_server::maybe_start();

    Ok(())
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
