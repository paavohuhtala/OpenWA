#![allow(non_snake_case)]

use std::ffi::c_void;

pub mod hook;
mod debug_ui;
mod replacements;
mod validation;

// ---------------------------------------------------------------------------
// DllMain
// ---------------------------------------------------------------------------

const DLL_PROCESS_ATTACH: u32 = 1;

/// Named event shared with the launcher. The launcher waits on this event
/// after DLL injection before resuming WA.exe's main thread, guaranteeing
/// all hooks are installed before any WA code runs.
const HOOKS_READY_EVENT: &[u8] = b"OpenWA_HooksReady\0";

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
    }
    1 // TRUE
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

pub use openwa_core::log::log_line;

fn clear_log() -> std::io::Result<()> {
    std::fs::write("OpenWA.log", "")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Main entry
// ---------------------------------------------------------------------------

fn run() -> Result<(), String> {
    let _ = clear_log();
    let _ = std::fs::write("OpenWA_validation.log", "");

    let delta = openwa_core::rebase::init();
    let _ = log_line(&format!(
        "=== OpenWA WormKit DLL loaded ===\n  ASLR delta: 0x{delta:08X}"
    ));

    replacements::install_all()?;

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

    Ok(())
}

/// Signal the `OpenWA_HooksReady` named event so the launcher knows it's
/// safe to resume WA.exe's main thread.
fn signal_hooks_ready() {
    use windows_sys::Win32::System::Threading::{OpenEventA, SetEvent};
    use windows_sys::Win32::Foundation::CloseHandle;

    const EVENT_MODIFY_STATE: u32 = 0x0002;

    unsafe {
        let handle = OpenEventA(
            EVENT_MODIFY_STATE,
            0, // bInheritHandle = FALSE
            HOOKS_READY_EVENT.as_ptr(),
        );
        if !handle.is_null() {
            SetEvent(handle);
            CloseHandle(handle);
            let _ = log_line("=== Signalled OpenWA_HooksReady ===");
        }
        // If handle is null, we weren't launched by our launcher — that's fine.
    }
}
