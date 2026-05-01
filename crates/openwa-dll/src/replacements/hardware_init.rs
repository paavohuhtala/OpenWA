//! Hook installation for `GameEngine__InitHardware` (0x0056D350).
//!
//! The implementation lives in `openwa_game::engine::hardware_init`. The only
//! caller — `GameSession::Run` (also fully replaced in Rust) — invokes
//! [`openwa_game::engine::hardware_init::init_hardware`] directly, so the
//! WA-side address is trapped as a safety net.

use crate::hook;
use openwa_game::address::va;
use openwa_game::engine::hardware_init::init_addrs;

pub fn install() -> Result<(), String> {
    unsafe {
        // Initialize bridge target addresses used by the Rust impl.
        init_addrs();

        // No remaining WA-side caller — `GameSession::Run` is fully Rust and
        // calls `init_hardware` directly.
        hook::install_trap!("GameEngine__InitHardware", va::GAME_ENGINE_INIT_HARDWARE);

        // Trap functions whose only caller was GameEngine__InitHardware (now Rust).
        hook::install_trap!("DSSound__Constructor", va::CONSTRUCT_DS_SOUND);
        hook::install_trap!("DSSOUND_INIT_BUFFERS", va::DSSOUND_INIT_BUFFERS);
    }
    Ok(())
}
