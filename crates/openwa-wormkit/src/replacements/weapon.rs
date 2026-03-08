//! Weapon ammo hooks.
//!
//! Replaces WA.exe functions that manage weapon ammo in the TeamArenaState area (DDGame + 0x4628):
//! - GetAmmo (0x5225E0): query ammo count with delay/phase checks
//! - AddAmmo (0x522640): add ammo to a weapon slot
//! - SubtractAmmo (0x522680): decrement ammo count
//! - CountAliveWorms (0x5225A0): check if >1 worm alive on team

use openwa_types::address::va;
use openwa_types::ddgame::{self, TeamArenaRef};
use openwa_types::weapon::Weapon;

use crate::hook::{self, usercall_trampoline};

// ============================================================
// AddAmmo replacement (0x522640)
// ============================================================
// __usercall: EAX = team_index, EDX = amount, [ESP+4] = team_info_base, [ESP+8] = weapon_id
// RET 0x8

unsafe extern "cdecl" fn add_ammo_impl(team_index: u32, amount: i32, arena: TeamArenaRef, weapon_id: u32) {
    let state = arena.state_mut();
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
// SubtractAmmo replacement (0x522680)
// ============================================================
// __usercall: EAX = team_index, ECX = team_info_base, [ESP+4] = weapon_id
// RET 0x4

unsafe extern "cdecl" fn subtract_ammo_impl(team_index: u32, arena: TeamArenaRef, weapon_id: u32) {
    let state = arena.state_mut();
    let idx = state.ammo_index(team_index as usize, weapon_id);
    let ammo = state.get_ammo(idx);
    if ammo > 0 {
        *state.ammo_mut(idx) = ammo - 1;
    }
}

usercall_trampoline!(fn trampoline_subtract_ammo; impl_fn = subtract_ammo_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

// ============================================================
// GetAmmo replacement (0x5225E0)
// ============================================================
// __usercall: EAX = team_index, ESI = team_info_base, EDX = weapon_id
// plain RET, returns EAX = ammo count

unsafe extern "cdecl" fn get_ammo_impl(team_index: u32, arena: TeamArenaRef, weapon_id: u32) -> u32 {
    let state = arena.state();
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
        if count_alive_worms_impl(team_index, arena) == 0 {
            return 0;
        }
    }

    state.get_ammo(idx) as u32
}

usercall_trampoline!(fn trampoline_get_ammo; impl_fn = get_ammo_impl;
    regs = [eax, esi, edx]);

// ============================================================
// CountAliveWorms replacement (0x5225A0)
// ============================================================
// __usercall: EAX = team_index, ECX = base
// plain RET, returns EAX = bool (1 if >1 worm alive on team)

unsafe extern "cdecl" fn count_alive_worms_impl(team_index: u32, arena: TeamArenaRef) -> u32 {
    let (block, sentinel) = arena.team_and_sentinel(team_index as usize);
    let worm_count = sentinel.sentinel_worm_count();
    let mut alive = 0i32;
    for w in 1..=worm_count as usize {
        if block.worms[w].health > 0 {
            alive += 1;
        }
    }
    if alive > 1 { 1 } else { 0 }
}

usercall_trampoline!(fn trampoline_count_alive_worms; impl_fn = count_alive_worms_impl;
    regs = [eax, ecx]);

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

        let _ = hook::install(
            "SubtractAmmo",
            va::SUBTRACT_AMMO,
            trampoline_subtract_ammo as *const (),
        )?;

        let _ = hook::install(
            "CountAliveWorms",
            va::COUNT_ALIVE_WORMS,
            trampoline_count_alive_worms as *const (),
        )?;
    }

    Ok(())
}
