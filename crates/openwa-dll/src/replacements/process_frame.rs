//! Hook for `GameSession__ProcessFrame` (0x572C80).
//!
//! Full replacement — game logic lives in `openwa_game::engine::process_frame`.

use openwa_game::address::va;

unsafe extern "C" fn hook_process_frame() {
    unsafe {
        openwa_game::engine::process_frame::process_frame();
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        openwa_game::engine::dispatch_frame::init_dispatch_addrs();

        if std::env::var("OPENWA_DISPATCH_ORIGINAL").is_ok() {
            openwa_game::engine::dispatch_frame::set_use_original_dispatch(true);
            let _ = crate::log_line(
                "[DispatchFrame] OPENWA_DISPATCH_ORIGINAL=1 — routing through vanilla 0x529160",
            );
        }
        if std::env::var("OPENWA_INTERP_LOG").is_ok() {
            openwa_game::engine::dispatch_frame::set_interp_log_enabled(true);
            let _ = crate::log_line(
                "[DispatchFrame] OPENWA_INTERP_LOG=1 — logging render_interp/accum snapshots",
            );
        }

        crate::hook::install(
            "GameSession__ProcessFrame",
            va::GAME_SESSION_PROCESS_FRAME,
            hook_process_frame as *const (),
        )?;
    }
    Ok(())
}
