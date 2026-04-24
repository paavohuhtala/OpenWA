//! Main loop hooks: `GameSession__ProcessFrame` replacement + traps on
//! `DDGameWrapper__DispatchFrame` and `DDGameWrapper__StepFrame`.
//!
//! ProcessFrame is fully replaced in Rust; its only downstream WA callees
//! (DispatchFrame, and StepFrame via DispatchFrame) are now unreachable.
//! The traps catch any regression that routes execution back into the
//! original WA implementations.

use crate::hook;
use openwa_game::address::va;

unsafe extern "C" fn hook_process_frame() {
    unsafe {
        openwa_game::engine::main_loop::process_frame::process_frame();
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        openwa_game::engine::main_loop::dispatch_frame::init_dispatch_addrs();
        hook::install(
            "GameSession__ProcessFrame",
            va::GAME_SESSION_PROCESS_FRAME,
            hook_process_frame as *const (),
        )?;
        hook::install_trap!(
            "DDGameWrapper__DispatchFrame",
            va::DDGAMEWRAPPER_DISPATCH_FRAME
        );
        hook::install_trap!("DDGameWrapper__StepFrame", va::DDGAMEWRAPPER_STEP_FRAME);
    }
    Ok(())
}
