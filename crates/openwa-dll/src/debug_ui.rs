//! Wrapper that spawns the openwa-debugui window when OPENWA_DEBUG_UI=1.
//!
//! Only compiled when the "debug-ui" feature is enabled.

#[cfg(feature = "debug-ui")]
pub fn maybe_spawn() {
    if std::env::var("OPENWA_DEBUG_UI").is_ok() {
        openwa_debugui::spawn();
    }
}

#[cfg(not(feature = "debug-ui"))]
pub fn maybe_spawn() {}
