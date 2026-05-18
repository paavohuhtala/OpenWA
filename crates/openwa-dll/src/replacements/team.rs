//! Team and worm state accessor hooks.
//!
//! Thin hook shim — all game logic lives in `openwa_game::engine::team_ops`.
//! All hooks are codegen-driven via `hooks/team.toml` + `re/**/*.toml`.

use openwa_game::engine::team_ops;
use openwa_game::engine::{TeamArena, TeamIndexMap};

use crate::generated::hooks;

// ── TeamArena__CountTeamsByAlliance (0x522030) ──

pub(crate) unsafe extern "cdecl" fn count_teams_by_alliance_impl(
    arena: *mut TeamArena,
    alliance_id: i32,
) {
    unsafe {
        team_ops::count_teams_by_alliance(arena, alliance_id);
    }
}

// ── TeamArena__GetTeamTotalHealth (0x5224D0) ──

pub(crate) unsafe extern "cdecl" fn get_team_total_health_impl(
    team_index: u32,
    arena: *mut TeamArena,
) -> i32 {
    unsafe { team_ops::get_team_total_health(team_index, arena) }
}

// ── TeamArena__IsWormInSpecialState (0x5226B0) ──

pub(crate) unsafe extern "cdecl" fn is_worm_in_special_state_impl(
    team_index: u32,
    worm_index: u32,
    arena: *mut TeamArena,
) -> u32 {
    unsafe { team_ops::is_worm_in_special_state(team_index, worm_index, arena) }
}

// ── TeamArena__GetWormPosition (0x522700) ──

pub(crate) unsafe extern "cdecl" fn get_worm_position_impl(
    team_index: u32,
    worm_index: u32,
    arena: *mut TeamArena,
    out_x: *mut i32,
    out_y: *mut i32,
) {
    unsafe {
        team_ops::get_worm_position(team_index, worm_index, arena, out_x, out_y);
    }
}

// ── TeamArena__CheckWormState0x64 (0x5228D0) ──

pub(crate) unsafe extern "cdecl" fn check_worm_state_0x64_impl(arena: *mut TeamArena) -> u32 {
    unsafe { team_ops::check_worm_state_0x64(arena) }
}

// ── TeamArena__CheckTeamWormState0x64 (0x522930) ──

pub(crate) unsafe extern "cdecl" fn check_team_worm_state_0x64_impl(
    arena: *mut TeamArena,
    team_idx: u32,
) -> u32 {
    unsafe { team_ops::check_team_worm_state_0x64(arena, team_idx) }
}

// ── TeamArena__CheckAnyWormState0x8b (0x522970) ──

pub(crate) unsafe extern "cdecl" fn check_any_worm_state_0x8b_impl(arena: *mut TeamArena) -> u32 {
    unsafe { team_ops::check_any_worm_state_0x8b(arena) }
}

// ── TeamIndexMap__RemoveHandle (0x00526000) ──

pub(crate) unsafe extern "cdecl" fn team_index_map_remove_handle_impl(
    map: *mut TeamIndexMap,
    handle: *mut i32,
) {
    unsafe {
        TeamIndexMap::remove_handle(map, handle);
    }
}

// ── TeamIndexMap__PopHandle (0x00525F50) ──
// `preserve_registers = ["ecx"]` in the hook TOML: MSVC callers of thiscall
// methods often loop without re-setting ECX between calls, relying on the
// callee to preserve it.

pub(crate) unsafe extern "cdecl" fn team_index_map_pop_handle_impl(
    map: *mut TeamIndexMap,
    key: i32,
) -> i32 {
    unsafe { TeamIndexMap::pop_handle(map, key) }
}

// ── TeamArena__SetActiveWorm (0x522500) ──

pub(crate) unsafe extern "cdecl" fn set_active_worm_impl(
    arena: *mut TeamArena,
    team_idx: u32,
    worm_index: i32,
) {
    unsafe {
        team_ops::set_active_worm(arena, team_idx, worm_index);
    }
}

// ── Hook installation ──

pub fn install() -> Result<(), String> {
    unsafe {
        hooks::install_TeamArena__CountTeamsByAlliance()?;
        hooks::install_TeamArena__GetTeamTotalHealth()?;
        hooks::install_TeamArena__IsWormInSpecialState()?;
        hooks::install_TeamArena__GetWormPosition()?;
        hooks::install_TeamArena__CheckWormState0x64()?;
        hooks::install_TeamArena__CheckTeamWormState0x64()?;
        hooks::install_TeamArena__CheckAnyWormState0x8b()?;
        hooks::install_TeamArena__SetActiveWorm()?;
        hooks::install_TeamIndexMap__RemoveHandle()?;
        hooks::install_TeamIndexMap__PopHandle()?;
    }
    Ok(())
}
