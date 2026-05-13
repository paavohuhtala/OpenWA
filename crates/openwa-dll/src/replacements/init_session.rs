//! Hook wiring for `GameInfo__InitSession`.
//!
//! Only the InitSession orchestrator is hooked. The two pure-logic helpers
//! (`Replay__ProcessSchemeDefaults`, `Replay__ProcessReplayFlags`) have Rust
//! ports in `openwa_game::engine::init_session` and they're called directly
//! from the Rust orchestrator — so any caller that goes through this hook
//! exercises them. We deliberately do NOT install MinHooks on those two
//! usercall functions, because the existing `engine::replay_loader` calls
//! them via `call_usercall_esi(gi, …)` with the wrong `gi`-vs-`prefix_ptr`
//! convention; hooking the WA addresses would route those bad calls into our
//! Rust ports and crash. See the comment block in `replay_loader.rs` near
//! the `process_colors` call site for the full story.

use openwa_game::address::va;
use openwa_game::engine::init_session as port;

use crate::hook;

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "GameInfo__InitSession",
            va::GAMEINFO_INIT_SESSION,
            port::init_session_shim as *const (),
        )?;
    }
    Ok(())
}
