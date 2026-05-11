//! Spawns the openwa-frontend match-launcher window when OPENWA_FRONTEND=1.
//!
//! Only compiled when the "match-launcher" feature is enabled.

#[cfg(feature = "match-launcher")]
pub fn maybe_spawn() {
    if std::env::var("OPENWA_FRONTEND").is_ok() {
        openwa_frontend::spawn();
    }
}

#[cfg(not(feature = "match-launcher"))]
pub fn maybe_spawn() {}
