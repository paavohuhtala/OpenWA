//! Team and worm state accessor hooks.
//!
//! Replaces WA.exe functions that access the TeamArenaState area (DDGame + 0x4628):
//! - CountTeamsByAlliance (0x522030): count teams by alliance membership
//! - GetTeamTotalHealth (0x5224D0): sum worm health for a team
//! - IsWormInSpecialState (0x5226B0): check worm state flag
//! - GetWormPosition (0x522700): read worm X,Y coordinates
//! - CheckWormState0x64 (0x5228D0): check if any worm has state 0x64
//! - CheckTeamWormState0x64 (0x522930): per-team version of above
//! - CheckAnyWormState0x8b (0x522970): scan all teams for state 0x8b
//! - SetActiveWorm_Maybe (0x522500): update team active state and counters

use openwa_core::address::va;
use openwa_core::ddgame::{self, TeamArenaRef};

use crate::hook::{self, usercall_trampoline};

// ============================================================
// CountTeamsByAlliance replacement (0x522030)
// ============================================================
// __usercall: EAX = base, EDI = alliance_id
// plain RET
//
// NOTE: This function uses Pattern B header fields (+0x70/+0x74) for
// alliance and active_worm, accessed via team_header_b().

unsafe extern "cdecl" fn count_teams_by_alliance_impl(arena: TeamArenaRef, alliance_id: i32) {
    let state = arena.state_mut();
    state.current_alliance = alliance_id;
    state.active_team_count = 0;
    state.same_alliance_count = 0;
    state.enemy_team_count = 0;

    for i in 0..state.team_count as usize {
        let header = arena.team_header_b(i);
        let team_alliance = header.alliance;
        let active_worm = header.active_worm;
        if active_worm != 0 && team_alliance >= 0 {
            state.active_team_count += 1;
            if team_alliance == alliance_id {
                state.same_alliance_count += 1;
            } else {
                state.enemy_team_count += 1;
            }
        }
    }
}

usercall_trampoline!(fn trampoline_count_teams_by_alliance; impl_fn = count_teams_by_alliance_impl;
    regs = [eax, edi]);

// ============================================================
// GetTeamTotalHealth replacement (0x5224D0)
// ============================================================
// __fastcall: ECX = team_index, EDX = base
// plain RET, returns EAX = total health

unsafe extern "cdecl" fn get_team_total_health_impl(team_index: u32, arena: TeamArenaRef) -> i32 {
    let header = arena.team_header(team_index as usize);

    if header.eliminated != 0 {
        return 0;
    }

    let worm_count = header.worm_count;
    let mut total = 0i32;
    for w in 1..=worm_count as usize {
        total += arena.team_worm(team_index as usize, w).health;
    }
    total
}

usercall_trampoline!(fn trampoline_get_team_total_health; impl_fn = get_team_total_health_impl;
    regs = [ecx, edx]);

// ============================================================
// IsWormInSpecialState replacement (0x5226B0)
// ============================================================
// __usercall: EAX = team_index, ECX = worm_index, [ESP+4] = base
// RET 0x4, returns EAX = bool (1 if special state)

unsafe extern "cdecl" fn is_worm_in_special_state_impl(
    team_index: u32, worm_index: u32, arena: TeamArenaRef,
) -> u32 {
    if ddgame::worm::is_special_state(arena.team_worm(team_index as usize, worm_index as usize).state) { 1 } else { 0 }
}

usercall_trampoline!(fn trampoline_is_worm_in_special_state; impl_fn = is_worm_in_special_state_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

// ============================================================
// GetWormPosition replacement (0x522700)
// ============================================================
// __usercall: EAX = team_index, ECX = worm_index,
//   [ESP+4] = base, [ESP+8] = out_x, [ESP+C] = out_y
// RET 0xC
//
// Reads pos_x/pos_y from WormEntry._unknown_90[0..4] / [4..8].
// These values appear transient — actual worm positions live in CGameTask objects.

unsafe extern "cdecl" fn get_worm_position_impl(
    team_index: u32, worm_index: u32, arena: TeamArenaRef, out_x: *mut i32, out_y: *mut i32,
) {
    let worm = arena.team_worm(team_index as usize, worm_index as usize);
    *out_x = *(worm._unknown_90.as_ptr() as *const i32);
    *out_y = *(worm._unknown_90.as_ptr().add(4) as *const i32);
}

usercall_trampoline!(fn trampoline_get_worm_position; impl_fn = get_worm_position_impl;
    regs = [eax, ecx]; stack_params = 3; ret_bytes = "0xC");

// ============================================================
// CheckWormState0x64 replacement (0x5228D0)
// ============================================================
// __usercall: EAX = base
// plain RET, returns EAX = bool (1 if any worm has state 0x64)
//
// Previously named "HasFullHealthWorm" but Ghidra disassembly confirms it
// reads worms[].state (offset +0x00), NOT worms[].health (offset +0x5C).
// It compares to 0x64 (100 decimal). State 0x64 is likely a transitional
// state distinct from 0x65 (idle). 11 xrefs in gameplay code.

unsafe extern "cdecl" fn check_worm_state_0x64_impl(arena: TeamArenaRef) -> u32 {
    let state = arena.state();

    // Iterate real teams (1-indexed: team 1..=team_count)
    for i in 1..=state.team_count as usize {
        let header = arena.team_header(i);
        if header.eliminated != 0 {
            continue;
        }
        let worm_count = header.worm_count;
        for w in 1..=worm_count as usize {
            if arena.team_worm(i, w).state == 0x64 {
                return 1;
            }
        }
    }
    0
}

usercall_trampoline!(fn trampoline_check_worm_state_0x64; impl_fn = check_worm_state_0x64_impl;
    reg = eax);

// ============================================================
// CheckTeamWormState0x64 replacement (0x522930)
// ============================================================
// __usercall: ECX = base, EAX = team_idx
// plain RET, returns EAX = bool (1 if any worm on team has state 0x64)
//
// Per-team version of CheckWormState0x64. 1 xref (FUN_00556ad0).

unsafe extern "cdecl" fn check_team_worm_state_0x64_impl(arena: TeamArenaRef, team_idx: u32) -> u32 {
    let header = arena.team_header(team_idx as usize);

    if header.eliminated != 0 {
        return 0;
    }

    let worm_count = header.worm_count;
    for w in 1..=worm_count as usize {
        if arena.team_worm(team_idx as usize, w).state == 0x64 {
            return 1;
        }
    }
    0
}

usercall_trampoline!(fn trampoline_check_team_worm_state_0x64; impl_fn = check_team_worm_state_0x64_impl;
    regs = [ecx, eax]);

// ============================================================
// CheckAnyWormState0x8b replacement (0x522970)
// ============================================================
// __usercall: EAX = base
// plain RET, returns EAX = bool (1 if any worm on any team has state 0x8b)
//
// Scans all teams. 1 xref (FUN_00557310).

unsafe extern "cdecl" fn check_any_worm_state_0x8b_impl(arena: TeamArenaRef) -> u32 {
    let state = arena.state();

    for i in 1..=state.team_count as usize {
        let header = arena.team_header(i);
        if header.eliminated != 0 {
            continue;
        }
        let worm_count = header.worm_count;
        for w in 1..=worm_count as usize {
            if arena.team_worm(i, w).state == 0x8b {
                return 1;
            }
        }
    }
    0
}

usercall_trampoline!(fn trampoline_check_any_worm_state_0x8b; impl_fn = check_any_worm_state_0x8b_impl;
    reg = eax);

// ============================================================
// SetActiveWorm_Maybe replacement (0x522500)
// ============================================================
// __usercall: EAX = base, EDX = team_idx (1-indexed), ESI = worm_index
// plain RET (4 RET sites in original)
//
// Sets the active worm for a team. worm_index=0 deactivates, worm_index=N
// sets worm N as active. Called on turn transitions and worm selection (Tab).
// Updates active counters and records last_active_team/alliance. 3 xrefs.

unsafe extern "cdecl" fn set_active_worm_impl(arena: TeamArenaRef, team_idx: u32, worm_index: i32) {
    let state = arena.state_mut();

    // Pattern B header: team_idx is 1-indexed, team_header_b is 0-indexed
    // team_idx=1 → team_header_b(0) → blocks[2].header
    let header = arena.team_header_b(team_idx as usize - 1);

    if worm_index == 0 {
        // Deactivate team
        if header.active_worm != 0 {
            state.active_worm_count -= 1;
            let alliance = header.alliance;
            if alliance >= 0 {
                state.active_team_count -= 1;
                if alliance == state.current_alliance {
                    state.same_alliance_count -= 1;
                } else {
                    state.enemy_team_count -= 1;
                }
            }
            arena.team_header_b_mut(team_idx as usize - 1).active_worm = 0;
        }
    } else {
        // Activate team — only update counters if not already active
        if header.active_worm == 0 {
            state.active_worm_count += 1;
            state.last_active_team = team_idx as i32;
            let alliance = header.alliance;
            state.last_active_alliance = alliance;
            if alliance >= 0 {
                state.active_team_count += 1;
                if alliance == state.current_alliance {
                    state.same_alliance_count += 1;
                } else {
                    state.enemy_team_count += 1;
                }
            }
        }
        // Always write worm_index (original writes ESI unconditionally)
        arena.team_header_b_mut(team_idx as usize - 1).active_worm = worm_index;
    }
}

usercall_trampoline!(fn trampoline_set_active_worm; impl_fn = set_active_worm_impl;
    regs = [eax, edx, esi]);

// ============================================================
// Hook installation
// ============================================================

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
