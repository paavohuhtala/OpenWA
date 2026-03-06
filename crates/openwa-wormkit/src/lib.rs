#![allow(non_snake_case)]

use std::ffi::c_void;

pub mod hook;
mod replacements;

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

    let delta = openwa_lib::rebase::init();
    let _ = log_line(&format!(
        "=== OpenWA WormKit DLL loaded ===\n  ASLR delta: 0x{delta:08X}"
    ));

    replacements::install_all()?;

    let _ = log_line("=== All replacements installed ===");
    Ok(())
}
