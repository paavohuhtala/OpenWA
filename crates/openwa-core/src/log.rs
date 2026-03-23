//! Shared logging — writes to `OpenWA.log` in the working directory,
//! or to the path specified by `OPENWA_LOG_PATH` env var.

use std::path::PathBuf;
use std::sync::OnceLock;

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

/// Append a line to the OpenWA log file.
pub fn log_line(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())?;
    writeln!(f, "{msg}")?;
    Ok(())
}
