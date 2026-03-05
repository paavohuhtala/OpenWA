#![allow(non_snake_case)]

use std::ffi::c_void;

const DLL_PROCESS_ATTACH: u32 = 1;

#[no_mangle]
unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        std::thread::spawn(|| {
            if let Err(e) = run_validation() {
                let _ = log_line(&format!("[FATAL] Validation failed to run: {}", e));
            }
        });
    }
    1 // TRUE
}

fn run_validation() -> Result<(), Box<dyn std::error::Error>> {
    let _ = log_line("=== OpenWA Validator ===");
    let _ = log_line("DLL loaded successfully. Validation will follow.");
    Ok(())
}

fn log_line(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("OpenWA_validation.log")?;
    writeln!(f, "{}", msg)?;
    Ok(())
}
