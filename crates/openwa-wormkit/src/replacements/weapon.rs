//! Weapon ammo hooks.
//!
//! Replaces WA.exe functions that manage weapon ammo in the TeamWeaponState area (DDGame + 0x4628):
//! - GetAmmo (0x5225E0): query ammo count with delay/phase checks
//! - AddAmmo (0x522640): add ammo to a weapon slot

use openwa_types::address::va;
use openwa_types::ddgame::{self, offsets, FullTeamBlock, TeamWeaponState};
use openwa_types::weapon::Weapon;

use crate::hook::{self, usercall_trampoline};

// ============================================================
// AddAmmo replacement (0x522640)
// ============================================================
// __usercall: EAX = team_index, EDX = amount, [ESP+4] = team_info_base, [ESP+8] = weapon_id
// RET 0x8

unsafe extern "cdecl" fn add_ammo_impl(team_index: u32, amount: i32, base: u32, weapon_id: u32) {
    let state = &mut *(base as *mut TeamWeaponState);
    let idx = state.ammo_index(team_index as usize, weapon_id);
    let ammo = state.get_ammo(idx);
    if ammo >= 0 {
        if amount < 0 {
            *state.ammo_mut(idx) = -1; // set unlimited
        } else {
            *state.ammo_mut(idx) = ammo + amount;
        }
    }
}

usercall_trampoline!(fn trampoline_add_ammo; impl_fn = add_ammo_impl;
    regs = [eax, edx]; stack_params = 2; ret_bytes = "0x8");

// ============================================================
// GetAmmo replacement (0x5225E0)
// ============================================================
// __usercall: EAX = team_index, ESI = team_info_base, EDX = weapon_id
// plain RET, returns EAX = ammo count

unsafe extern "cdecl" fn get_ammo_impl(team_index: u32, base: u32, weapon_id: u32) -> u32 {
    let state = &*(base as *const TeamWeaponState);
    let idx = state.ammo_index(team_index as usize, weapon_id);

    // Check weapon delay
    if state.get_delay(idx) != 0 {
        if state.game_mode_flag == 0 {
            return 0;
        }
        // In sudden death (phase >= 484), delayed weapons return 0
        // unless it's Teleport (weapon 0x28)
        if state.game_phase >= ddgame::GAME_PHASE_SUDDEN_DEATH && weapon_id != Weapon::Teleport as u32 {
            return 0;
        }
    }

    // SelectWorm (0x3B) requires >1 alive worm on the team
    if state.game_phase >= ddgame::GAME_PHASE_NORMAL_MIN && weapon_id == Weapon::SelectWorm as u32 {
        if !count_alive_worms(state, team_index as usize) {
            return 0;
        }
    }

    state.get_ammo(idx) as u32
}

/// Returns true if more than 1 worm is alive on the team (health > 0).
unsafe fn count_alive_worms(state: &TeamWeaponState, team_index: usize) -> bool {
    let base = state as *const TeamWeaponState as *const u8;
    let blocks = base.sub(offsets::TWS_TO_BLOCKS) as *const FullTeamBlock;
    let block = &*blocks.add(team_index);
    let sentinel = &(*blocks.add(team_index + 1)).worms[0];
    let worm_count = sentinel.sentinel_worm_count();
    let mut alive = 0i32;
    for w in 1..=worm_count as usize {
        if block.worms[w].health > 0 {
            alive += 1;
        }
    }
    alive > 1
}

usercall_trampoline!(fn trampoline_get_ammo; impl_fn = get_ammo_impl;
    regs = [eax, esi, edx]);

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install(
            "AddAmmo",
            va::ADD_AMMO,
            trampoline_add_ammo as *const (),
        )?;

        let _ = hook::install(
            "GetAmmo",
            va::GET_AMMO,
            trampoline_get_ammo as *const (),
        )?;
    }

    Ok(())
}
