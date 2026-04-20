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
        crate::hook::install(
            "GameSession__ProcessFrame",
            va::GAME_SESSION_PROCESS_FRAME,
            hook_process_frame as *const (),
        )?;
    }
    Ok(())
}
