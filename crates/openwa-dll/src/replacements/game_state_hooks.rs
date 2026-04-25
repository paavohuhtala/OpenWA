//! Passthrough hooks for GameWorld__InitGameState sub-functions.
//!
//! These sub-constructors are called from the Rust port of InitGameState
//! via transmute bridges. Passthrough hooks remain for logging. Functions
//! that use implicit register params (usercall) have no hooks installed
//! to avoid dropping ECX/EDX/ESI.

use crate::hook;
use crate::log_line;
use openwa_game::address::va;

// ─── Trampoline storage ─────────────────────────────────────────────────────

static mut HUD_PANEL_ORIG: *const () = core::ptr::null();
static mut INIT_WEAPON_TABLE_ORIG: *const () = core::ptr::null();
static mut INIT_TEAMS_ORIG: *const () = core::ptr::null();
static mut TEAM_MANAGER_ORIG: *const () = core::ptr::null();
static mut GAME_STATE_ORIG: *const () = core::ptr::null();

// ─── Passthrough hooks ──────────────────────────────────────────────────────

// HudPanel__Constructor (0x524070): stdcall(this), RET 0x4
unsafe extern "stdcall" fn hook_hud_panel(this: u32) -> u32 {
    unsafe {
        let _ = log_line(&format!(
            "[InitGameState] HudPanel::Constructor this=0x{this:08X}"
        ));
        let orig: unsafe extern "stdcall" fn(u32) -> u32 = core::mem::transmute(HUD_PANEL_ORIG);
        orig(this)
    }
}

// InitWeaponTable (0x53CAB0): stdcall(runtime), RET 0x4
unsafe extern "stdcall" fn hook_init_weapon_table(wrapper: u32) -> u32 {
    unsafe {
        let _ = log_line(&format!(
            "[InitGameState] InitWeaponTable wrapper=0x{wrapper:08X}"
        ));
        let orig: unsafe extern "stdcall" fn(u32) -> u32 =
            core::mem::transmute(INIT_WEAPON_TABLE_ORIG);
        orig(wrapper)
    }
}

// GameWorld__InitTeamsFromSetup (0x5220B0): stdcall(team_arena, setup_data), RET 0x8
unsafe extern "stdcall" fn hook_init_teams(team_arena: u32, setup_data: u32) -> u32 {
    unsafe {
        let _ = log_line(&format!(
            "[InitGameState] InitTeamsFromSetup arena=0x{team_arena:08X} setup=0x{setup_data:08X}"
        ));
        let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 =
            core::mem::transmute(INIT_TEAMS_ORIG);
        orig(team_arena, setup_data)
    }
}

// TeamManager__Constructor (0x563D40): stdcall(this, wrapper), RET 0x8
unsafe extern "stdcall" fn hook_team_manager(this: u32, wrapper: u32) -> u32 {
    unsafe {
        let _ = log_line(&format!(
            "[InitGameState] TeamManager::Constructor this=0x{this:08X} wrapper=0x{wrapper:08X}"
        ));
        let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 =
            core::mem::transmute(TEAM_MANAGER_ORIG);
        orig(this, wrapper)
    }
}

// GameStateEntity__Constructor / SerializeGameState (0x532330): stdcall(this, param), RET 0x8
unsafe extern "stdcall" fn hook_game_state(this: u32, param: u32) -> u32 {
    unsafe {
        let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 =
            core::mem::transmute(GAME_STATE_ORIG);
        orig(this, param)
    }
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        // Pure stdcall hooks — safe to intercept, log, and forward.
        HUD_PANEL_ORIG = hook::install(
            "HudPanel__Constructor",
            va::HUD_PANEL_CONSTRUCTOR,
            hook_hud_panel as *const (),
        )? as *const ();

        INIT_WEAPON_TABLE_ORIG = hook::install(
            "InitWeaponTable",
            va::INIT_WEAPON_TABLE,
            hook_init_weapon_table as *const (),
        )? as *const ();

        INIT_TEAMS_ORIG = hook::install(
            "GameWorld__InitTeamsFromSetup",
            va::INIT_TEAMS_FROM_SETUP,
            hook_init_teams as *const (),
        )? as *const ();

        TEAM_MANAGER_ORIG = hook::install(
            "TeamManager__Constructor",
            va::TEAM_MANAGER_CONSTRUCTOR,
            hook_team_manager as *const (),
        )? as *const ();

        GAME_STATE_ORIG = hook::install(
            "GameStateEntity__Constructor",
            va::GAME_STATE_CONSTRUCTOR,
            hook_game_state as *const (),
        )? as *const ();

        // Usercall functions (ECX/EDX/ESI carry implicit params) are NOT hooked.
        // Passthrough hooks would drop register values. These are called directly
        // from the Rust InitGameState port via typed transmute/fastcall bridges:
        //   - WorldRootEntity__Constructor (ECX=GameWorld)
        //   - DisplayGfx__ConstructFull (fastcall: ECX+EDX)
        //   - DisplayGfx__ConstructTextbox (thiscall: ECX=display)
        //   - DisplayObject__Constructor (fastcall: ECX+EDX)
        //   - GameWorld__InitWeaponPanel (usercall: ESI=this)
        //   - BufferObject__Constructor, GameStateStream__Init (pure stdcall,
        //     but hooks removed since only caller is Rust)
    }

    Ok(())
}
