//! Hook for `GameSession__ProcessFrame` (0x572C80).
//!
//! Full replacement — game logic lives in `openwa_core::engine::process_frame`.

use openwa_core::address::va;

unsafe extern "C" fn hook_process_frame() {
    openwa_core::engine::process_frame::process_frame();
}

pub fn install() -> Result<(), String> {
    unsafe {
        crate::hook::install(
            "GameSession__ProcessFrame",
            va::GAME_SESSION_PROCESS_FRAME,
            hook_process_frame as *const (),
        )?;
    }
    Ok(())
}
