//! Shared logging — writes to `OpenWA.log` in the working directory,
//! or to the path specified by `OPENWA_LOG_PATH` env var. Also tees to an
//! optional secondary sink registered by the host crate (the launcher pipe).

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Resolve the log file path once. Uses `OPENWA_LOG_PATH` env var if set,
/// otherwise defaults to `"OpenWA.log"`.
fn log_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        std::env::var_os("OPENWA_LOG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("OpenWA.log"))
    })
}

/// Optional secondary sink. The DLL initializes this with a writer that
/// forwards lines over an anonymous pipe to the launcher; the launcher's
/// reader thread prints them on its own stdout. Cross-process stdio
/// inheritance is unreliable for GUI-subsystem children writing through
/// conpty handles (output is silently dropped on most modern terminal
/// hosts), so the pipe is the only mechanism that works in practice.
static SECONDARY_SINK: Mutex<Option<Box<dyn Write + Send>>> = Mutex::new(None);

/// Register a secondary sink for log output. Called once during DLL startup.
/// The previous sink (if any) is dropped.
pub fn set_secondary_sink(sink: Box<dyn Write + Send>) {
    if let Ok(mut guard) = SECONDARY_SINK.lock() {
        *guard = Some(sink);
    }
}

/// Append a line to the OpenWA log file, and best-effort tee it to the
/// secondary sink if one is registered.
pub fn log_line(msg: &str) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())?;
    writeln!(f, "{msg}")?;
    if let Ok(mut guard) = SECONDARY_SINK.lock()
        && let Some(sink) = guard.as_mut()
    {
        let _ = writeln!(sink, "{msg}");
    }
    Ok(())
}
