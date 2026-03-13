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

pub fn log_line(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("OpenWA.log")?;
    writeln!(f, "{msg}")?;
    Ok(())
}

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
