//! Team / alliance / scoring initialization phases of `init_game_state`.
//!
//! Pure Rust ports of three sub-functions called once during GameWorld setup:
//! - `WorldEntity__InitTeamScoring` (0x528510)
//! - `WorldEntity__InitAllianceData` (0x5262D0)
//! - the team-name CPU-marker check baked into `GameWorld__InitGameState`

use crate::engine::game_info::GameInfo;
use crate::engine::runtime::GameRuntime;
use crate::engine::world::GameWorld;

/// Maximum number of teams (13 slots: 6 real + sentinels/padding).
const MAX_TEAMS: usize = 13;

/// Team entry stride in GameInfo (3000 bytes per team slot).
const TEAM_ENTRY_STRIDE: usize = 3000;

/// Pure Rust implementation of WorldEntity_InitTeamScoring_Maybe (0x528510).
///
/// Convention: fastcall(ECX=wrapper), plain RET.
///
/// Initializes 7 parallel u32 arrays (13 elements each) in WorldEntity, then
/// sets the starting-team flag, initializes team task pointer flags based on
/// game mode (training vs normal), and zeros BaseEntity sub-object fields.
pub unsafe fn init_team_scoring(runtime: *mut GameRuntime) {
    unsafe {
        let game_info = (*(*runtime).world).game_info;

        let scoring_param_a = (*game_info).scoring_param_a as u32;
        let scoring_param_b = (*game_info).scoring_param_b as u32;
        let value_a = scoring_param_a * 50;
        let value_b = scoring_param_b * 50;

        for i in 0..MAX_TEAMS {
            (*runtime).team_score_array_0[i] = 0;
            (*runtime).team_starting_marker[i] = 0;
            (*runtime).team_scoring_a[i] = value_a;
            (*runtime).team_score_array_3[i] = 0;
            (*runtime).team_scoring_b[i] = value_b;
            (*runtime).team_scoring_c[i] = value_b;
            (*runtime).team_score_array_6[i] = 1;
        }

        let starting_team = (*game_info).starting_team_index as usize;
        (*runtime).team_starting_marker[starting_team] = 1;

        let mode_flag = (*game_info).game_mode_flag;
        let num_teams = (*game_info).num_teams as i32;

        if mode_flag < 0 {
            for i in 0..num_teams as usize {
                (*runtime).team_activity_flags[i] = 0xFFFFFFFE;
            }

            let alliance_team_count = (*game_info).team_record_count as i32;
            if alliance_team_count > 0 {
                let mut offset: usize = 0;
                for _ in 0..alliance_team_count {
                    let alliance_group =
                        *((game_info as *const u8).add(0x450 + offset) as *const i8) as i32;
                    if alliance_group >= 0 {
                        (*runtime).team_activity_flags[alliance_group as usize] = 0;
                    }
                    offset += TEAM_ENTRY_STRIDE;
                }
            }
        } else {
            for i in 0..num_teams as usize {
                (*runtime).team_activity_flags[i] = 0xFFFFFFFF;
            }

            let starting_flag_team = (*game_info).game_mode_flag as i32;
            (*runtime).team_activity_flags[starting_flag_team as usize] = 1;
        }

        if num_teams > 0 {
            for i in 0..num_teams as usize {
                let task = (*runtime).team_task_ptrs[i];
                if !task.is_null() {
                    *(task.add(0x18) as *mut u32) = 0;
                    *(task.add(0x14) as *mut u32) = 0;
                    *(task.add(0x10) as *mut u32) = 0;
                    *(task.add(0x0C) as *mut u32) = 0;
                    *(task.add(0x08) as *mut u32) = 0;
                }
            }
        }
    }
}

/// Pure Rust implementation of WorldEntity_InitAllianceData_Maybe (0x5262D0).
///
/// Convention: usercall(EAX=wrapper), plain RET.
///
/// Initializes the 13-entry alliance bitmask table on the wrapper, the
/// 6-entry per-team scoring flags on GameWorld, and an auxiliary alliance ID
/// array (`slot_to_team`) for non-starting-team alliances.
pub unsafe fn init_alliance_data(runtime: *mut GameRuntime) {
    unsafe {
        (*runtime)._alliance_bitmasks = [0u32; 13];

        let world = (*runtime).world;
        let game_info = (*world).game_info;
        let num_teams = (*game_info).num_teams as i32;
        let alliance_team_count = (*game_info).team_record_count as i32;

        if num_teams != 0 && alliance_team_count > 0 {
            let mut offset: usize = 0;
            for _ in 0..alliance_team_count {
                let alliance_group =
                    *((game_info as *const u8).add(0x450 + offset) as *const i8) as i32;
                let alliance_id = *((game_info as *const u8).add(0x451 + offset));

                if alliance_group >= 0 {
                    (*runtime)._alliance_bitmasks[alliance_group as usize] |=
                        1u32 << (alliance_id & 0x1F);
                }

                offset += TEAM_ENTRY_STRIDE;
            }
        }

        (*world).team_scoring_flags = [0u32; 6];

        let mut seen: [u8; 6] = [0; 6];
        let mut aux_count: usize = 0;

        if alliance_team_count > 0 {
            let mut team_offset: usize = 0;

            for team_idx in 0..alliance_team_count {
                let alliance_group =
                    *((game_info as *const u8).add(0x450 + team_offset) as *const i8) as i32;
                let alliance_id = *((game_info as *const u8).add(0x451 + team_offset)) as u32;

                let scoring_flag =
                    core::ptr::addr_of_mut!((*world).team_scoring_flags[team_idx as usize]);

                if alliance_group < 0 {
                    let override_val = (*runtime).replay_flag_a as i8 as i32;
                    *scoring_flag = override_val as u32;
                } else {
                    let num_teams_byte = (*game_info).num_teams;
                    let override_byte = (*runtime).replay_flag_a;

                    if num_teams_byte == 0 || override_byte != 0 {
                        *scoring_flag = 1;
                    } else {
                        let game_version = (*game_info).game_version;
                        let starting_team = (*game_info).starting_team_index as i32;

                        let team_bitmask: u32 = if game_version < 0x83 {
                            (*runtime)._alliance_bitmasks[alliance_group as usize]
                        } else {
                            1u32 << (alliance_id & 0x1F)
                        };

                        let starting_bitmask =
                            (*runtime)._alliance_bitmasks[starting_team as usize];

                        if (starting_bitmask & team_bitmask) != 0 {
                            *scoring_flag = 1;
                        }
                    }

                    let starting_team2 = (*game_info).starting_team_index as i32;

                    if alliance_group != starting_team2 && seen[alliance_id as usize] == 0 {
                        let starting_bitmask2 =
                            (*runtime)._alliance_bitmasks[starting_team2 as usize];
                        if (starting_bitmask2 & (1u32 << (alliance_id & 0x1F))) != 0 {
                            (*runtime).slot_to_team[aux_count] = (alliance_id + 0x10) as i32;
                            aux_count += 1;
                            seen[alliance_id as usize] = 1;
                        }
                    }
                }

                team_offset += TEAM_ENTRY_STRIDE;
            }
        }
    }
}

/// Validate team names — check for CPU team markers (high bit of name first byte).
///
/// Inlined within the original `GameWorld__InitGameState` (0x526500). Sets
/// `GameWorld.team_color` from the GameInfo source byte, then overrides to -1
/// if any team name's first byte has bit 7 set (CPU marker).
pub unsafe fn init_team_color_from_names(world: *mut GameWorld, game_info: *mut GameInfo) {
    unsafe {
        (*world).team_color = (*game_info).team_color_source as u32;

        let team_count = (*game_info).team_record_count;
        let gi_raw = game_info as *const u8;
        if team_count != 0 {
            for i in 0..team_count as usize {
                let name_byte = *(gi_raw.add(0x450 + i * TEAM_ENTRY_STRIDE));
                if name_byte >= 0x80 {
                    (*world).team_color = 0xFFFFFFFF;
                    break;
                }
            }
        }
    }
}
