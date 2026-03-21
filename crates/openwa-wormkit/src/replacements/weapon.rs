//! Weapon hooks.
//!
//! Replaces WA.exe functions that manage weapon ammo in the TeamArenaState area (DDGame + 0x4628):
//! - GetAmmo (0x5225E0): query ammo count with delay/phase checks
//! - AddAmmo (0x522640): add ammo to a weapon slot
//! - SubtractAmmo (0x522680): decrement ammo count
//! - CountAliveWorms (0x5225A0): check if >1 worm alive on team
//! - FireWeapon (0x51EE60): passthrough with weapon type logging

use core::sync::atomic::{AtomicU32, Ordering};

use openwa_core::address::va;
use openwa_core::engine::ddgame::{self, TeamArenaRef};
use openwa_core::game::Weapon;
use openwa_core::log::log_line;

use crate::hook::{self, usercall_trampoline};

// ============================================================
// AddAmmo replacement (0x522640)
// ============================================================
// __usercall: EAX = team_index, EDX = amount, [ESP+4] = team_info_base, [ESP+8] = weapon_id
// RET 0x8

unsafe extern "cdecl" fn add_ammo_impl(
    team_index: u32,
    amount: i32,
    arena: TeamArenaRef,
    weapon_id: u32,
) {
    let idx = arena.ammo_index(team_index as usize, weapon_id);
    let state = arena.state_mut();
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
    let idx = arena.ammo_index(team_index as usize, weapon_id);
    let state = arena.state_mut();
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

unsafe extern "cdecl" fn get_ammo_impl(
    team_index: u32,
    arena: TeamArenaRef,
    weapon_id: u32,
) -> u32 {
    let idx = arena.ammo_index(team_index as usize, weapon_id);
    let state = arena.state();

    // Check weapon delay
    if state.get_delay(idx) != 0 {
        if state.game_mode_flag == 0 {
            return 0;
        }
        // In sudden death (phase >= 484), delayed weapons return 0
        // unless it's Teleport (weapon 0x28)
        if state.game_phase >= ddgame::GAME_PHASE_SUDDEN_DEATH
            && weapon_id != Weapon::Teleport as u32
        {
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
    let header = arena.team_header(team_index as usize);
    let worm_count = header.worm_count;
    let mut alive = 0i32;
    for w in 1..=worm_count as usize {
        if arena.team_worm(team_index as usize, w).health > 0 {
            alive += 1;
        }
    }
    if alive > 1 {
        1
    } else {
        0
    }
}

usercall_trampoline!(fn trampoline_count_alive_worms; impl_fn = count_alive_worms_impl;
    regs = [eax, ecx]);

// ============================================================
// FireWeapon passthrough (0x51EE60)
// ============================================================
// usercall(EAX=weapon_ctx) + 1 stack param (wrapper), RET 0x4.
// weapon_ctx+0x30 = weapon type (1-4)
// weapon_ctx+0x34 = subtype for types 3,4
// weapon_ctx+0x38 = subtype for types 1,2

static ORIG_FIRE_WEAPON: AtomicU32 = AtomicU32::new(0);

/// Logger called from the naked passthrough.
/// `weapon_ctx` = EAX = pointer to weapon data (from CTaskWorm+0x36C).
/// `worm_ptr` = stack param = CTaskWorm pointer (pushed by WeaponRelease).
unsafe extern "cdecl" fn fire_weapon_log(weapon_ctx: u32, worm_ptr: u32) {
    let ctx = weapon_ctx as *const u8;
    let weapon_type = *(ctx.add(0x30) as *const i32);
    let subtype_34 = *(ctx.add(0x34) as *const i32);
    let subtype_38 = *(ctx.add(0x38) as *const i32);

    // Read selected weapon from CTaskWorm+0x170
    let weapon_id = *((worm_ptr as *const u8).add(0x170) as *const u32);
    let weapon_name = Weapon::try_from(weapon_id)
        .map(|w| format!("{:?}", w))
        .unwrap_or_else(|id| format!("Unknown({})", id));

    let _ = log_line(&format!(
        "[Weapon] FireWeapon: {} (id={}) type={} sub34={} sub38={}",
        weapon_name, weapon_id, weapon_type, subtype_34, subtype_38
    ));
}

/// Naked passthrough: save regs → call logger → restore → jmp original.
/// Stack layout at entry: [ret_addr] [wrapper_param]
/// We read wrapper_param to pass to the logger as the worm pointer.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_fire_weapon() {
    core::arch::naked_asm!(
        "push eax",
        "push ecx",
        "push edx",
        // call logger(weapon_ctx=EAX, worm_ptr=[ESP+16])
        // ESP+16 because: 3 pushes (12 bytes) + ret_addr (4) = 16 to stack param
        "push [esp+16]",
        "push eax",
        "call {log_fn}",
        "add esp, 8",
        "pop edx",
        "pop ecx",
        "pop eax",
        "jmp [{orig}]",
        log_fn = sym fire_weapon_log,
        orig = sym ORIG_FIRE_WEAPON,
    );
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install("AddAmmo", va::ADD_AMMO, trampoline_add_ammo as *const ())?;

        let _ = hook::install("GetAmmo", va::GET_AMMO, trampoline_get_ammo as *const ())?;

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

        let trampoline = hook::install(
            "FireWeapon",
            va::FIRE_WEAPON,
            trampoline_fire_weapon as *const (),
        )?;
        ORIG_FIRE_WEAPON.store(trampoline as u32, Ordering::Relaxed);
    }

    Ok(())
}
