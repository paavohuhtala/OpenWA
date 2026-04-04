//! Team and worm state accessor hooks.
//!
//! Thin hook shim — all game logic lives in `openwa_core::engine::team_ops`.
//! This file contains only usercall trampolines and hook installation.

use openwa_core::engine::team_ops;
use openwa_core::{address::va, engine::TeamArenaRef};

use crate::hook::{self, usercall_trampoline};

// ── CountTeamsByAlliance (0x522030): usercall(EAX=base, EDI=alliance_id) ──

unsafe extern "cdecl" fn count_teams_by_alliance_impl(arena: TeamArenaRef, alliance_id: i32) {
    team_ops::count_teams_by_alliance(arena, alliance_id);
}

usercall_trampoline!(fn trampoline_count_teams_by_alliance; impl_fn = count_teams_by_alliance_impl;
    regs = [eax, edi]);

// ── GetTeamTotalHealth (0x5224D0): fastcall(ECX=team_index, EDX=base) ──

unsafe extern "cdecl" fn get_team_total_health_impl(team_index: u32, arena: TeamArenaRef) -> i32 {
    team_ops::get_team_total_health(team_index, arena)
}

usercall_trampoline!(fn trampoline_get_team_total_health; impl_fn = get_team_total_health_impl;
    regs = [ecx, edx]);

// ── IsWormInSpecialState (0x5226B0): usercall(EAX=team, ECX=worm, [ESP+4]=base) ──

unsafe extern "cdecl" fn is_worm_in_special_state_impl(
    team_index: u32,
    worm_index: u32,
    arena: TeamArenaRef,
) -> u32 {
    team_ops::is_worm_in_special_state(team_index, worm_index, arena)
}

usercall_trampoline!(fn trampoline_is_worm_in_special_state; impl_fn = is_worm_in_special_state_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

// ── GetWormPosition (0x522700): usercall(EAX=team, ECX=worm, stack=base,x,y) ──

unsafe extern "cdecl" fn get_worm_position_impl(
    team_index: u32,
    worm_index: u32,
    arena: TeamArenaRef,
    out_x: *mut i32,
    out_y: *mut i32,
) {
    team_ops::get_worm_position(team_index, worm_index, arena, out_x, out_y);
}

usercall_trampoline!(fn trampoline_get_worm_position; impl_fn = get_worm_position_impl;
    regs = [eax, ecx]; stack_params = 3; ret_bytes = "0xC");

// ── CheckWormState0x64 (0x5228D0): usercall(EAX=base) ──

unsafe extern "cdecl" fn check_worm_state_0x64_impl(arena: TeamArenaRef) -> u32 {
    team_ops::check_worm_state_0x64(arena)
}

usercall_trampoline!(fn trampoline_check_worm_state_0x64; impl_fn = check_worm_state_0x64_impl;
    reg = eax);

// ── CheckTeamWormState0x64 (0x522930): usercall(ECX=base, EAX=team_idx) ──

unsafe extern "cdecl" fn check_team_worm_state_0x64_impl(
    arena: TeamArenaRef,
    team_idx: u32,
) -> u32 {
    team_ops::check_team_worm_state_0x64(arena, team_idx)
}

usercall_trampoline!(fn trampoline_check_team_worm_state_0x64; impl_fn = check_team_worm_state_0x64_impl;
    regs = [ecx, eax]);

// ── CheckAnyWormState0x8b (0x522970): usercall(EAX=base) ──

unsafe extern "cdecl" fn check_any_worm_state_0x8b_impl(arena: TeamArenaRef) -> u32 {
    team_ops::check_any_worm_state_0x8b(arena)
}

usercall_trampoline!(fn trampoline_check_any_worm_state_0x8b; impl_fn = check_any_worm_state_0x8b_impl;
    reg = eax);

// ── SetActiveWorm_Maybe (0x522500): usercall(EAX=base, EDX=team_idx, ESI=worm) ──

unsafe extern "cdecl" fn set_active_worm_impl(arena: TeamArenaRef, team_idx: u32, worm_index: i32) {
    team_ops::set_active_worm(arena, team_idx, worm_index);
}

usercall_trampoline!(fn trampoline_set_active_worm; impl_fn = set_active_worm_impl;
    regs = [eax, edx, esi]);

// ── Hook installation ──

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install(
            "CountTeamsByAlliance",
            va::COUNT_TEAMS_BY_ALLIANCE,
            trampoline_count_teams_by_alliance as *const (),
        )?;
        let _ = hook::install(
            "GetTeamTotalHealth",
            va::GET_TEAM_TOTAL_HEALTH,
            trampoline_get_team_total_health as *const (),
        )?;
        let _ = hook::install(
            "IsWormInSpecialState",
            va::IS_WORM_IN_SPECIAL_STATE,
            trampoline_is_worm_in_special_state as *const (),
        )?;
        let _ = hook::install(
            "GetWormPosition",
            va::GET_WORM_POSITION,
            trampoline_get_worm_position as *const (),
        )?;
        let _ = hook::install(
            "CheckWormState0x64",
            va::CHECK_WORM_STATE_0X64,
            trampoline_check_worm_state_0x64 as *const (),
        )?;
        let _ = hook::install(
            "CheckTeamWormState0x64",
            va::CHECK_TEAM_WORM_STATE_0X64,
            trampoline_check_team_worm_state_0x64 as *const (),
        )?;
        let _ = hook::install(
            "CheckAnyWormState0x8b",
            va::CHECK_ANY_WORM_STATE_0X8B,
            trampoline_check_any_worm_state_0x8b as *const (),
        )?;
        let _ = hook::install(
            "SetActiveWorm_Maybe",
            va::SET_ACTIVE_WORM_MAYBE,
            trampoline_set_active_worm as *const (),
        )?;
    }
    Ok(())
}
