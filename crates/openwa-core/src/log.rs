//! Shared logging — writes to `OpenWA.log` in the working directory.

/// Append a line to `OpenWA.log`.
pub fn log_line(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("OpenWA.log")?;
    writeln!(f, "{msg}")?;
    Ok(())
}
