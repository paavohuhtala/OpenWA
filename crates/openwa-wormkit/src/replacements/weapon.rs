//! Weapon ammo hooks.
//!
//! Replaces WA.exe functions that manage weapon ammo in the TeamWeaponState area (DDGame + 0x4628):
//! - GetAmmo (0x5225E0): query ammo count with delay/phase checks
//! - AddAmmo (0x522640): add ammo to a weapon slot

use openwa_types::address::va;
use openwa_types::ddgame::{self, TeamWeaponState};
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

/// Returns true if more than 1 worm is alive on the team.
///
/// Worm data lives at negative offsets from the TeamEntry:
/// - worm_count at team_entry_addr - 0x4
/// - worm array at team_entry_addr - 0x4A0 (stride 0x9C, health at [0])
unsafe fn count_alive_worms(state: &TeamWeaponState, team_index: usize) -> bool {
    let base = state as *const TeamWeaponState as *const u8;
    let team_entry = base.add(team_index * 0x51C) as usize;
    let worm_count = *((team_entry - ddgame::worm::COUNT_OFFSET) as *const i32);
    let mut alive = 0i32;
    if worm_count > 0 {
        let mut ptr = (team_entry - ddgame::worm::ARRAY_OFFSET) as *const u8;
        for _ in 0..worm_count {
            if *(ptr as *const i32) > 0 {
                alive += 1;
            }
            ptr = ptr.add(ddgame::worm::STRIDE);
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
