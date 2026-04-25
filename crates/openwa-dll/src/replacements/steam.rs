//! Steamworks SDK bypass.
//!
//! `Steam__Bootstrap` (0x00598D40) initializes the Steamworks SDK and verifies
//! ownership; its sole caller, `Frontend__MainNavigationLoop`, exits silently
//! when it returns 0 (Steam not running, app not owned, or restart triggered).
//!
//! Setting `OPENWA_NO_STEAM=1` replaces the wrapper with a stub that returns 1
//! unconditionally, allowing WA.exe to run without Steam — used in CI where the
//! Steam client cannot be installed. The Steam overlay and friend-name lookup
//! become unavailable, but `Frontend__GetUserName` (0x004A8A90) already falls
//! back to `GetUserNameA`/`GetComputerNameA` when `SteamFriends()` is null.

use crate::log_line;
use openwa_game::address::va;

unsafe extern "C" fn steam_bootstrap_stub() -> u32 {
    let _ = log_line("[Steam] Bootstrap bypassed (OPENWA_NO_STEAM=1)");
    1
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_NO_STEAM").is_err() {
        return Ok(());
    }

    unsafe {
        let _ = crate::hook::install(
            "Steam__Bootstrap",
            va::STEAM_BOOTSTRAP,
            steam_bootstrap_stub as *const (),
        )?;
    }
    Ok(())
}
