//! Per-frame game state checksum logging for desync bisection.
//!
//! When `OPENWA_TRACE_DESYNC=1`, hooks GameFrameChecksumProcessor (0x5329C0)
//! to capture WA's own network sync checksums after each frame.
//!
//! The checksum processor is `__thiscall` (ECX=controller, stack=DDGameWrapper*),
//! calls SerializeGameState + ComputeStateChecksum, and stores results at
//! wrapper+0x268 (checksum_a) and wrapper+0x26c (checksum_b).
//!
//! Output: one line per frame in the hash log file:
//!   frame_number<TAB>checksum_a_hex<TAB>checksum_b_hex

use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::engine::DDGameWrapper;

/// Trampoline to the original GameFrameChecksumProcessor.
static ORIG_CHECKSUM_PROCESSOR: AtomicU32 = AtomicU32::new(0);

/// Buffered writer for the hash log file. Protected by mutex but only accessed
/// from the game's main thread.
static HASH_LOG: Mutex<Option<std::io::BufWriter<std::fs::File>>> = Mutex::new(None);

/// Passthrough hook for GameFrameChecksumProcessor (0x5329C0).
///
/// `__thiscall`: ECX = controller object, stack param = DDGameWrapper*.
/// After calling the original, reads the computed checksums and logs them.
unsafe extern "thiscall" fn hook_checksum_processor(ctrl: u32, wrapper: *mut DDGameWrapper) {
    // Call original
    let orig: unsafe extern "thiscall" fn(u32, *mut DDGameWrapper) =
        core::mem::transmute(ORIG_CHECKSUM_PROCESSOR.load(Ordering::Relaxed));
    orig(ctrl, wrapper);

    // Read checksums written by the original function
    if wrapper.is_null() {
        return;
    }
    let ddgame = (*wrapper).ddgame;
    if ddgame.is_null() {
        return;
    }

    let frame = (*ddgame).frame_counter;
    let checksum_a = (*wrapper).sync_checksum_a;
    let checksum_b = (*wrapper).sync_checksum_b;

    if let Ok(mut guard) = HASH_LOG.lock() {
        if let Some(writer) = guard.as_mut() {
            let _ = writeln!(writer, "{}\t{:08X}\t{:08X}", frame, checksum_a, checksum_b);
        }
    }
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_TRACE_DESYNC").is_err() {
        return Ok(());
    }

    // Determine log file path
    let path =
        std::env::var("OPENWA_TRACE_HASH_PATH").unwrap_or_else(|_| "frame_hashes.log".to_string());

    let file = std::fs::File::create(&path)
        .map_err(|e| format!("Failed to create hash log {path}: {e}"))?;
    *HASH_LOG.lock().unwrap() = Some(std::io::BufWriter::new(file));

    let _ = log_line(&format!("[TraceDesync] Logging frame hashes to {path}"));

    unsafe {
        let trampoline = hook::install(
            "GameFrameChecksumProcessor",
            va::GAME_FRAME_CHECKSUM_PROCESSOR,
            hook_checksum_processor as *const (),
        )?;
        ORIG_CHECKSUM_PROCESSOR.store(trampoline as u32, Ordering::Relaxed);
    }

    Ok(())
}

/// Flush and close the hash log. Called from DLL_PROCESS_DETACH via
/// `write_gameplay_report()`.
pub fn flush() {
    if let Ok(mut guard) = HASH_LOG.lock() {
        if let Some(writer) = guard.as_mut() {
            let _ = writer.flush();
        }
        *guard = None;
    }
}
