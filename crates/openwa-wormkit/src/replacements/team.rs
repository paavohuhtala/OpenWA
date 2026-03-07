//! Team and worm state accessor hooks.
//!
//! Replaces WA.exe functions that access the TeamWeaponState area (DDGame + 0x4628):
//! - CountTeamsByAlliance (0x522030): count teams by alliance membership
//! - GetTeamTotalHealth (0x5224D0): sum worm health for a team
//! - IsWormInSpecialState (0x5226B0): check worm state flag
//! - GetWormPosition (0x522700): read worm X,Y coordinates
//! - CheckWormState0x64 (0x5228D0): check if any worm has state 0x64

use openwa_types::address::va;
use openwa_types::ddgame::{self, offsets, FullTeamBlock, TeamWeaponState};

use crate::hook::{self, usercall_trampoline};

// ============================================================
// CountTeamsByAlliance replacement (0x522030)
// ============================================================
// __usercall: EAX = base, EDI = alliance_id
// plain RET
//
// NOTE: This function uses a DIFFERENT sentinel layout than entry_ptr-based
// functions. It reads alliance/alive at TWS+0x510+i*0x51C (= sentinel+0x70/0x74),
// not the entry_ptr-based sentinel+0x78/0x80. Left as raw pointer math until
// the +0x70/+0x74 sentinel fields are better understood.

unsafe extern "cdecl" fn count_teams_by_alliance_impl(base: u32, alliance_id: i32) {
    let state = &mut *(base as *mut TeamWeaponState);
    state.current_alliance = alliance_id;
    state.active_team_count = 0;
    state.same_alliance_count = 0;
    state.enemy_team_count = 0;

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
    let blocks = (base as *const u8).sub(offsets::TWS_TO_BLOCKS) as *const FullTeamBlock;
    let block = &*blocks.add(team_index as usize);
    let sentinel = &(*blocks.add(team_index as usize + 1)).worms[0];

    if sentinel.sentinel_eliminated() != 0 {
        return 0;
    }

    let worm_count = sentinel.sentinel_worm_count();
    let mut total = 0i32;
    for w in 1..=worm_count as usize {
        total += block.worms[w].health;
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
    let blocks = (base as *const u8).sub(offsets::TWS_TO_BLOCKS) as *const FullTeamBlock;
    let block = &*blocks.add(team_index as usize);
    if ddgame::worm::is_special_state(block.worms[worm_index as usize].state) { 1 } else { 0 }
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
    team_index: u32, worm_index: u32, base: u32, out_x: *mut i32, out_y: *mut i32,
) {
    let blocks = (base as *const u8).sub(offsets::TWS_TO_BLOCKS) as *const FullTeamBlock;
    let block = &*blocks.add(team_index as usize);
    let worm = &block.worms[worm_index as usize];
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

unsafe extern "cdecl" fn check_worm_state_0x64_impl(base: u32) -> u32 {
    let state = &*(base as *const TeamWeaponState);
    let blocks = (base as *const u8).sub(offsets::TWS_TO_BLOCKS) as *const FullTeamBlock;

    // Iterate real teams (1-indexed: team 1..=team_count)
    for i in 1..=state.team_count as usize {
        let sentinel = &(*blocks.add(i + 1)).worms[0];
        if sentinel.sentinel_eliminated() != 0 {
            continue;
        }
        let worm_count = sentinel.sentinel_worm_count();
        let worm_block = &*blocks.add(i);
        for w in 1..=worm_count as usize {
            if worm_block.worms[w].state == 0x64 {
                return 1;
            }
        }
    }
    0
}

usercall_trampoline!(fn trampoline_check_worm_state_0x64; impl_fn = check_worm_state_0x64_impl;
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
            "CheckWormState0x64",
            va::CHECK_WORM_STATE_0X64,
            trampoline_check_worm_state_0x64 as *const (),
        )?;
    }

    Ok(())
}
