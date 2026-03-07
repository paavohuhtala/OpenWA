//! Team and worm state accessor hooks.
//!
//! Replaces WA.exe functions that access the TeamWeaponState area (DDGame + 0x4628):
//! - CountTeamsByAlliance (0x522030): count teams by alliance membership
//! - GetTeamTotalHealth (0x5224D0): sum worm health for a team
//! - IsWormInSpecialState (0x5226B0): check worm state flag
//! - GetWormPosition (0x522700): read worm X,Y coordinates
//! - HasFullHealthWorm (0x5228D0): check if any worm has full health

use openwa_types::address::va;
use openwa_types::ddgame::{self, TeamWeaponState};

use crate::hook::{self, usercall_trampoline};

// ============================================================
// CountTeamsByAlliance replacement (0x522030)
// ============================================================
// __usercall: EAX = base, EDI = alliance_id
// plain RET

unsafe extern "cdecl" fn count_teams_by_alliance_impl(base: u32, alliance_id: i32) {
    let state = &mut *(base as *mut TeamWeaponState);
    state.current_alliance = alliance_id;
    state.active_team_count = 0;
    state.same_alliance_count = 0;
    state.enemy_team_count = 0;

    // Teams are 1-indexed; iterate 1..=team_count
    // Per-team data: base + 0x510 + (team_1based - 1) * 0x51C
    // At [+0]: alliance field, [+4]: alive flag
    let base_ptr = base as *const u8;
    for i in 0..state.team_count {
        let team_data = base_ptr.add(ddgame::team_data::BASE_OFFSET + i as usize * 0x51C);
        let team_alliance = *(team_data as *const i32);
        let alive_flag = *(team_data.add(ddgame::team_data::ALIVE_FLAG) as *const i32);
        if alive_flag != 0 && team_alliance >= 0 {
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

unsafe extern "cdecl" fn get_team_total_health_impl(team_index: u32, base: u32) -> i32 {
    let base_ptr = base as *const u8;
    let team_entry = base_ptr.add(team_index as usize * 0x51C) as usize;

    // Check eliminated flag at team_entry - ELIMINATED_OFFSET
    if *((team_entry - ddgame::team_data::ELIMINATED_OFFSET) as *const i32) != 0 {
        return 0;
    }

    let worm_count = *((team_entry - ddgame::worm::COUNT_OFFSET) as *const i32);
    let mut total = 0i32;
    if worm_count > 0 {
        let mut ptr = (team_entry - ddgame::worm::ARRAY_OFFSET) as *const u8;
        for _ in 0..worm_count {
            total += *(ptr as *const i32);
            ptr = ptr.add(ddgame::worm::STRIDE);
        }
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
    team_index: u32, worm_index: u32, base: u32,
) -> u32 {
    let addr = base as usize
        + team_index as usize * 0x51C
        + worm_index as usize * ddgame::worm::STRIDE;
    let state_val = *((addr.wrapping_sub(ddgame::worm::STATE_OFFSET)) as *const u32);
    if ddgame::worm::is_special_state(state_val) { 1 } else { 0 }
}

usercall_trampoline!(fn trampoline_is_worm_in_special_state; impl_fn = is_worm_in_special_state_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

// ============================================================
// GetWormPosition replacement (0x522700)
// ============================================================
// __usercall: EAX = team_index, ECX = worm_index,
//   [ESP+4] = base, [ESP+8] = out_x, [ESP+C] = out_y
// RET 0xC

unsafe extern "cdecl" fn get_worm_position_impl(
    team_index: u32, worm_index: u32, base: u32, out_x: *mut i32, out_y: *mut i32,
) {
    let addr = base as usize
        + team_index as usize * 0x51C
        + worm_index as usize * ddgame::worm::STRIDE;
    *out_x = *((addr.wrapping_sub(ddgame::worm::POS_X_OFFSET)) as *const i32);
    *out_y = *((addr.wrapping_sub(ddgame::worm::POS_Y_OFFSET)) as *const i32);
}

usercall_trampoline!(fn trampoline_get_worm_position; impl_fn = get_worm_position_impl;
    regs = [eax, ecx]; stack_params = 3; ret_bytes = "0xC");

// ============================================================
// HasFullHealthWorm replacement (0x5228D0)
// ============================================================
// __usercall: EAX = base
// plain RET, returns EAX = bool (1 if any team has a worm at full health)

unsafe extern "cdecl" fn has_full_health_worm_impl(base: u32) -> u32 {
    let state = &*(base as *const TeamWeaponState);
    let base_ptr = base as *const u8;

    for i in 0..state.team_count {
        let team_ptr = base_ptr.add(ddgame::team_data::ALT_BASE_OFFSET + i as usize * 0x51C);
        if *(team_ptr.sub(ddgame::team_data::ALT_ELIMINATED_OFFSET) as *const i32) != 0 {
            continue;
        }
        let worm_count = *(team_ptr as *const i32);
        if worm_count > 0 {
            let mut worm_ptr = team_ptr.sub(ddgame::team_data::ALT_WORM_ARRAY_OFFSET);
            for _ in 0..worm_count {
                if *(worm_ptr as *const i32) == ddgame::worm::FULL_HEALTH {
                    return 1;
                }
                worm_ptr = worm_ptr.add(ddgame::worm::STRIDE);
            }
        }
    }
    0
}

usercall_trampoline!(fn trampoline_has_full_health_worm; impl_fn = has_full_health_worm_impl;
    reg = eax);

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
            "HasFullHealthWorm",
            va::HAS_FULL_HEALTH_WORM,
            trampoline_has_full_health_worm as *const (),
        )?;
    }

    Ok(())
}
