//! Spawns the openwa-frontend match-launcher window when OPENWA_FRONTEND=1,
//! and (optionally) arms the GameInfo watchpoints on WA's main thread when
//! OPENWA_WATCH_GAMEINFO=1.

#[cfg(feature = "match-launcher")]
pub fn maybe_spawn() {
    if std::env::var("OPENWA_FRONTEND").is_ok() {
        openwa_frontend::spawn();
    }
}

#[cfg(not(feature = "match-launcher"))]
pub fn maybe_spawn() {}

/// Register the GameInfo-watchpoint-arming callback so the egui frontend
/// can trigger it later via `openwa_game::main_thread::request_arm_*`.
/// Called once at DLL startup; cheap.
pub fn register_arm_gameinfo_watchpoints() {
    openwa_game::main_thread::register_arm_gameinfo_watchpoints(arm_gameinfo_main_thread);
}

extern "C" fn arm_gameinfo_main_thread() {
    use openwa_game::address::va;
    use openwa_game::rebase::rb;
    unsafe {
        crate::debug_watchpoint::prepare();
        let base = rb(va::G_GAME_INFO) as *mut u8;
        crate::debug_watchpoint::on_base_known(base);
    }
}
