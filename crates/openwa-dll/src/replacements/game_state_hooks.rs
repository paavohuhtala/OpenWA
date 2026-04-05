//! Passthrough hooks for DDGame__InitGameState sub-functions.
//!
//! These hooks intercept stdcall constructors called during InitGameState,
//! log their parameters, call the original, and return. This validates
//! calling conventions and prepares for future Rust replacements.

use crate::hook;
use crate::log_line;
use openwa_core::address::va;

// ─── Trampoline storage ─────────────────────────────────────────────────────

static mut HUD_PANEL_ORIG: *const () = core::ptr::null();
static mut INIT_WEAPON_TABLE_ORIG: *const () = core::ptr::null();
static mut INIT_TEAMS_ORIG: *const () = core::ptr::null();
static mut TEAM_MANAGER_ORIG: *const () = core::ptr::null();
static mut TURN_GAME_ORIG: *const () = core::ptr::null();
static mut GAME_STATE_ORIG: *const () = core::ptr::null();
static mut DISPLAYGFX_FULL_ORIG: *const () = core::ptr::null();
static mut TEXTBOX_ORIG: *const () = core::ptr::null();
static mut WEAPON_PANEL_ORIG: *const () = core::ptr::null();
static mut BUFFER_OBJ_ORIG: *const () = core::ptr::null();
static mut STREAM_INIT_ORIG: *const () = core::ptr::null();
static mut DISPLAY_OBJ_ORIG: *const () = core::ptr::null();

// ─── Passthrough hooks ──────────────────────────────────────────────────────

// HudPanel__Constructor (0x524070): stdcall(this), RET 0x4
unsafe extern "stdcall" fn hook_hud_panel(this: u32) -> u32 {
    let _ = log_line(&format!(
        "[InitGameState] HudPanel::Constructor this=0x{this:08X}"
    ));
    let orig: unsafe extern "stdcall" fn(u32) -> u32 = core::mem::transmute(HUD_PANEL_ORIG);
    orig(this)
}

// InitWeaponTable (0x53CAB0): stdcall(wrapper), RET 0x4
unsafe extern "stdcall" fn hook_init_weapon_table(wrapper: u32) -> u32 {
    let _ = log_line(&format!(
        "[InitGameState] InitWeaponTable wrapper=0x{wrapper:08X}"
    ));
    let orig: unsafe extern "stdcall" fn(u32) -> u32 = core::mem::transmute(INIT_WEAPON_TABLE_ORIG);
    orig(wrapper)
}

// DDGame__InitTeamsFromSetup (0x5220B0): stdcall(team_arena, setup_data), RET 0x8
unsafe extern "stdcall" fn hook_init_teams(team_arena: u32, setup_data: u32) -> u32 {
    let _ = log_line(&format!(
        "[InitGameState] InitTeamsFromSetup arena=0x{team_arena:08X} setup=0x{setup_data:08X}"
    ));
    let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 = core::mem::transmute(INIT_TEAMS_ORIG);
    orig(team_arena, setup_data)
}

// TeamManager__Constructor (0x563D40): stdcall(this, wrapper), RET 0x8
unsafe extern "stdcall" fn hook_team_manager(this: u32, wrapper: u32) -> u32 {
    let _ = log_line(&format!(
        "[InitGameState] TeamManager::Constructor this=0x{this:08X} wrapper=0x{wrapper:08X}"
    ));
    let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 = core::mem::transmute(TEAM_MANAGER_ORIG);
    orig(this, wrapper)
}

// CTaskTurnGame__Constructor (0x55B280): stdcall(this, setup_data), RET 0x8
// No logging — format! clobbers ECX which the post-call code may depend on
unsafe extern "stdcall" fn hook_turn_game(this: u32, setup_data: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 = core::mem::transmute(TURN_GAME_ORIG);
    orig(this, setup_data)
}

// CTaskGameState__Constructor (0x532330): stdcall(this, param), RET 0x8
// No logging — may clobber registers the caller depends on
unsafe extern "stdcall" fn hook_game_state(this: u32, param: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 = core::mem::transmute(GAME_STATE_ORIG);
    orig(this, param)
}

// ─── Group F: Display objects ────────────────────────────────────────────────

// DisplayGfx__ConstructFull (0x563FC0): stdcall(5 params), RET 0x14
unsafe extern "stdcall" fn hook_displaygfx_full(
    p1: u32,
    p2: u32,
    p3: u32,
    p4: u32,
    p5: u32,
) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32, u32, u32) -> u32 =
        core::mem::transmute(DISPLAYGFX_FULL_ORIG);
    orig(p1, p2, p3, p4, p5)
}

// DisplayGfx__ConstructTextbox (0x4FAF00): stdcall(3 params), RET 0xC
unsafe extern "stdcall" fn hook_textbox(p1: u32, p2: u32, p3: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32) -> u32 = core::mem::transmute(TEXTBOX_ORIG);
    orig(p1, p2, p3)
}

// DDGame__InitWeaponPanel (0x567770): stdcall(1 param = wrapper), RET 0x4
unsafe extern "stdcall" fn hook_weapon_panel(wrapper: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32) -> u32 = core::mem::transmute(WEAPON_PANEL_ORIG);
    orig(wrapper)
}

// ─── Group G: Buffer/stream objects ─────────────────────────────────────────

// BufferObject__Constructor (0x545FD0): stdcall(3 params), RET 0xC
unsafe extern "stdcall" fn hook_buffer_obj(p1: u32, p2: u32, p3: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32) -> u32 =
        core::mem::transmute(BUFFER_OBJ_ORIG);
    orig(p1, p2, p3)
}

// GameStateStream__Init (0x4FB490): stdcall(1 param), RET 0x4
unsafe extern "stdcall" fn hook_stream_init(p1: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32) -> u32 = core::mem::transmute(STREAM_INIT_ORIG);
    orig(p1)
}

// DisplayObject__Constructor (0x540440): stdcall(2 params), RET 0x8
unsafe extern "stdcall" fn hook_display_obj(p1: u32, p2: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32) -> u32 = core::mem::transmute(DISPLAY_OBJ_ORIG);
    orig(p1, p2)
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
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
            "DDGame__InitTeamsFromSetup",
            va::INIT_TEAMS_FROM_SETUP,
            hook_init_teams as *const (),
        )? as *const ();

        TEAM_MANAGER_ORIG = hook::install(
            "TeamManager__Constructor",
            va::TEAM_MANAGER_CONSTRUCTOR,
            hook_team_manager as *const (),
        )? as *const ();

        TURN_GAME_ORIG = hook::install(
            "CTaskTurnGame__Constructor",
            va::TURN_GAME_CONSTRUCTOR,
            hook_turn_game as *const (),
        )? as *const ();
        GAME_STATE_ORIG = hook::install(
            "CTaskGameState__Constructor",
            va::GAME_STATE_CONSTRUCTOR,
            hook_game_state as *const (),
        )? as *const ();

        // Group F: Display objects
        DISPLAYGFX_FULL_ORIG = hook::install(
            "DisplayGfx__ConstructFull",
            va::DISPLAYGFX_CONSTRUCT_FULL,
            hook_displaygfx_full as *const (),
        )? as *const ();

        TEXTBOX_ORIG = hook::install(
            "DisplayGfx__ConstructTextbox",
            va::CONSTRUCT_TEXTBOX,
            hook_textbox as *const (),
        )? as *const ();

        WEAPON_PANEL_ORIG = hook::install(
            "DDGame__InitWeaponPanel",
            va::FUN_567770,
            hook_weapon_panel as *const (),
        )? as *const ();

        // Group G: Buffer/stream objects
        BUFFER_OBJ_ORIG = hook::install(
            "BufferObject__Constructor",
            va::BUFFER_OBJECT_CONSTRUCTOR,
            hook_buffer_obj as *const (),
        )? as *const ();

        STREAM_INIT_ORIG = hook::install(
            "GameStateStream__Init",
            va::GAME_STATE_STREAM_INIT,
            hook_stream_init as *const (),
        )? as *const ();

        DISPLAY_OBJ_ORIG = hook::install(
            "DisplayObject__Constructor",
            va::DISPLAY_OBJECT_CONSTRUCTOR,
            hook_display_obj as *const (),
        )? as *const ();
    }

    Ok(())
}
