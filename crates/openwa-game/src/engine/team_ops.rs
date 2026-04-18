//! Team and worm state accessor functions.
//!
//! Pure Rust reimplementations of WA.exe functions that access the TeamArena
//! area (DDGame + 0x4628). Called from hook trampolines in openwa-dll.
//!
//! Original WA functions:
//! - CountTeamsByAlliance (0x522030)
//! - GetTeamTotalHealth (0x5224D0)
//! - IsWormInSpecialState (0x5226B0)
//! - GetWormPosition (0x522700)
//! - CheckWormState0x64 (0x5228D0)
//! - CheckTeamWormState0x64 (0x522930)
//! - CheckAnyWormState0x8b (0x522970)
//! - SetActiveWorm_Maybe (0x522500)

use crate::engine::team_arena::worm;

use super::team_arena::TeamArena;

/// Count teams by alliance membership — port of 0x522030.
///
/// Updates `state.active_team_count`, `same_alliance_count`, `enemy_team_count`.
/// Uses Pattern B header fields (+0x70/+0x74) for alliance and active_worm.
pub unsafe fn count_teams_by_alliance(arena: *mut TeamArena, alliance_id: i32) {
    unsafe {
        (*arena).current_alliance = alliance_id;
        (*arena).active_team_count = 0;
        (*arena).same_alliance_count = 0;
        (*arena).enemy_team_count = 0;

        for i in 0..(*arena).team_count as usize {
            let header = TeamArena::team_header_b(arena, i);
            let team_alliance = (*header).alliance;
            let active_worm = (*header).active_worm;
            if active_worm != 0 && team_alliance >= 0 {
                (*arena).active_team_count += 1;
                if team_alliance == alliance_id {
                    (*arena).same_alliance_count += 1;
                } else {
                    (*arena).enemy_team_count += 1;
                }
            }
        }
    }
}

/// Sum worm health for a team — port of 0x5224D0.
///
/// Returns 0 if the team is eliminated.
pub unsafe fn get_team_total_health(team_index: u32, arena: *const TeamArena) -> i32 {
    unsafe {
        let header = TeamArena::team_header(arena, team_index as usize);

        if (*header).eliminated != 0 {
            return 0;
        }

        let worm_count = (*header).worm_count;
        let mut total = 0i32;
        for w in 1..=worm_count as usize {
            total += (*TeamArena::team_worm(arena, team_index as usize, w)).health;
        }
        total
    }
}

/// Check worm state flag — port of 0x5226B0.
///
/// Returns 1 if the worm is in a "special" state, 0 otherwise.
pub unsafe fn is_worm_in_special_state(
    team_index: u32,
    worm_index: u32,
    arena: *const TeamArena,
) -> u32 {
    unsafe {
        if worm::is_special_state(
            (*TeamArena::team_worm(arena, team_index as usize, worm_index as usize)).state,
        ) {
            1
        } else {
            0
        }
    }
}

/// Read worm X,Y coordinates — port of 0x522700.
///
/// Reads pos_x/pos_y from WormEntry._unknown_90[0..4] / [4..8].
/// These values appear transient — actual worm positions live in CGameTask objects.
pub unsafe fn get_worm_position(
    team_index: u32,
    worm_index: u32,
    arena: *const TeamArena,
    out_x: *mut i32,
    out_y: *mut i32,
) {
    unsafe {
        let worm = TeamArena::team_worm(arena, team_index as usize, worm_index as usize);
        *out_x = *((*worm)._unknown_90.as_ptr() as *const i32);
        *out_y = *((*worm)._unknown_90.as_ptr().add(4) as *const i32);
    }
}

/// Check if any worm on any team has state 0x64 — port of 0x5228D0.
///
/// Iterates real teams (1-indexed). 11 xrefs in gameplay code.
pub unsafe fn check_worm_state_0x64(arena: *const TeamArena) -> u32 {
    unsafe {
        for i in 1..=(*arena).team_count as usize {
            let header = TeamArena::team_header(arena, i);
            if (*header).eliminated != 0 {
                continue;
            }
            let worm_count = (*header).worm_count;
            for w in 1..=worm_count as usize {
                if (*TeamArena::team_worm(arena, i, w)).state == 0x64 {
                    return 1;
                }
            }
        }
        0
    }
}

/// Check if any worm on a specific team has state 0x64 — port of 0x522930.
///
/// Per-team version. 1 xref (FUN_00556ad0).
pub unsafe fn check_team_worm_state_0x64(arena: *const TeamArena, team_idx: u32) -> u32 {
    unsafe {
        let header = TeamArena::team_header(arena, team_idx as usize);

        if (*header).eliminated != 0 {
            return 0;
        }

        let worm_count = (*header).worm_count;
        for w in 1..=worm_count as usize {
            if (*TeamArena::team_worm(arena, team_idx as usize, w)).state == 0x64 {
                return 1;
            }
        }
        0
    }
}

/// Scan all teams for state 0x8b — port of 0x522970.
///
/// 1 xref (FUN_00557310).
pub unsafe fn check_any_worm_state_0x8b(arena: *const TeamArena) -> u32 {
    unsafe {
        for i in 1..=(*arena).team_count as usize {
            let header = TeamArena::team_header(arena, i);
            if (*header).eliminated != 0 {
                continue;
            }
            let worm_count = (*header).worm_count;
            for w in 1..=worm_count as usize {
                if (*TeamArena::team_worm(arena, i, w)).state == 0x8b {
                    return 1;
                }
            }
        }
        0
    }
}

/// Set the active worm for a team — port of 0x522500.
///
/// `worm_index=0` deactivates the team. `worm_index=N` sets worm N as active.
/// Called on turn transitions and worm selection (Tab).
/// Updates active counters and records last_active_team/alliance.
/// Uses Pattern B headers (0-indexed from `team_idx - 1`).
pub unsafe fn set_active_worm(arena: *mut TeamArena, team_idx: u32, worm_index: i32) {
    unsafe {
        // Pattern B header: team_idx is 1-indexed, team_header_b is 0-indexed
        // team_idx=1 → team_header_b(0) → blocks[2].header
        let header = TeamArena::team_header_b_mut(arena, team_idx as usize - 1);

        if worm_index == 0 {
            // Deactivate team
            if (*header).active_worm != 0 {
                (*arena).active_worm_count -= 1;
                let alliance = (*header).alliance;
                if alliance >= 0 {
                    (*arena).active_team_count -= 1;
                    if alliance == (*arena).current_alliance {
                        (*arena).same_alliance_count -= 1;
                    } else {
                        (*arena).enemy_team_count -= 1;
                    }
                }
                (*header).active_worm = 0;
            }
        } else {
            // Activate team — only update counters if not already active
            if (*header).active_worm == 0 {
                (*arena).active_worm_count += 1;
                (*arena).last_active_team = team_idx as i32;
                let alliance = (*header).alliance;
                (*arena).last_active_alliance = alliance;
                if alliance >= 0 {
                    (*arena).active_team_count += 1;
                    if alliance == (*arena).current_alliance {
                        (*arena).same_alliance_count += 1;
                    } else {
                        (*arena).enemy_team_count += 1;
                    }
                }
            }
            // Always write worm_index (original writes ESI unconditionally)
            (*header).active_worm = worm_index;
        }
    }
}
