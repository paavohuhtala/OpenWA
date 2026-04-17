//! Weapon hooks.
//!
//! Thin hook shim — all game logic lives in `openwa_game::game::weapon_fire`.
//! This file contains usercall trampolines, passthrough hooks, and installation.
//!
//! Hooks:
//! - AddAmmo (0x522640), SubtractAmmo (0x522680), GetAmmo (0x5225E0)
//! - CountAliveWorms (0x5225A0)
//! - FireWeapon (0x51EE60): trapped (called directly from weapon_release)
//! - CreateWeaponProjectile (0x51E0F0), ProjectileFire (0x51DFB0), CreateArrow (0x51ED90)
//! - StrikeFire (0x51E2C0), PlacedExplosive (0x51EC80): passthrough (log + call original)

use core::sync::atomic::{AtomicU32, Ordering};

use openwa_game::game::weapon::WeaponFireParams;
use openwa_game::game::weapon_fire;
use openwa_game::log::log_line;
use openwa_game::task::worm::CTaskWorm;
use openwa_game::{address::va, engine::TeamArena};

use crate::hook::{self, usercall_trampoline};

// ── AddAmmo (0x522640): usercall(EAX=team, EDX=amount, stack=arena,weapon_id) ──

unsafe extern "cdecl" fn add_ammo_impl(
    team_index: u32,
    amount: i32,
    arena: *mut TeamArena,
    weapon_id: u32,
) {
    unsafe {
        weapon_fire::add_ammo(team_index, amount, arena, weapon_id);
    }
}

usercall_trampoline!(fn trampoline_add_ammo; impl_fn = add_ammo_impl;
    regs = [eax, edx]; stack_params = 2; ret_bytes = "0x8");

// ── SubtractAmmo (0x522680): usercall(EAX=team, ECX=arena, stack=weapon_id) ──

unsafe extern "cdecl" fn subtract_ammo_impl(
    team_index: u32,
    arena: *mut TeamArena,
    weapon_id: u32,
) {
    unsafe {
        weapon_fire::subtract_ammo(team_index, arena, weapon_id);
    }
}

usercall_trampoline!(fn trampoline_subtract_ammo; impl_fn = subtract_ammo_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

// ── GetAmmo (0x5225E0): usercall(EAX=team, ESI=arena, EDX=weapon_id) ──

unsafe extern "cdecl" fn get_ammo_impl(
    team_index: u32,
    arena: *mut TeamArena,
    weapon_id: u32,
) -> u32 {
    unsafe { weapon_fire::get_ammo(team_index, arena, weapon_id) }
}

usercall_trampoline!(fn trampoline_get_ammo; impl_fn = get_ammo_impl;
    regs = [eax, esi, edx]);

// ── CountAliveWorms (0x5225A0): usercall(EAX=team, ECX=arena) ──

unsafe extern "cdecl" fn count_alive_worms_impl(team_index: u32, arena: *mut TeamArena) -> u32 {
    unsafe { weapon_fire::count_alive_worms(team_index, arena) }
}

usercall_trampoline!(fn trampoline_count_alive_worms; impl_fn = count_alive_worms_impl;
    regs = [eax, ecx]);

// ── Passthrough hooks (log + call original) ──

static ORIG_STRIKE_FIRE: AtomicU32 = AtomicU32::new(0);
static ORIG_PLACED_EXPLOSIVE: AtomicU32 = AtomicU32::new(0);

// ── CreateWeaponProjectile (0x51E0F0): thiscall(ECX=worm, fire_params, local_struct) ──

unsafe extern "thiscall" fn hook_create_weapon_projectile(
    worm: *mut CTaskWorm,
    fire_params: *const WeaponFireParams,
    local_struct: *const u8,
) {
    unsafe {
        weapon_fire::create_weapon_projectile(worm, fire_params, local_struct);
    }
}

// ── ProjectileFire (0x51DFB0): stdcall(worm, fire_params, local_struct) ──

unsafe extern "stdcall" fn hook_projectile_fire(
    worm: *mut CTaskWorm,
    fire_params: *const WeaponFireParams,
    local_struct: *const openwa_game::game::weapon::WeaponSpawnData,
) {
    unsafe {
        weapon_fire::projectile_fire(worm, fire_params, local_struct);
    }
}

// ── CreateArrow (0x51ED90): thiscall(ECX=worm, fire_params, local_struct) ──

unsafe extern "thiscall" fn hook_create_arrow(
    worm: *mut CTaskWorm,
    fire_params: *const WeaponFireParams,
    local_struct: *const u8,
) {
    unsafe {
        weapon_fire::create_arrow(worm, fire_params, local_struct);
    }
}

// ── StrikeFire (0x51E2C0): passthrough (log + call original) ──

#[unsafe(naked)]
unsafe extern "C" fn trampoline_strike_fire() {
    core::arch::naked_asm!(
        "push eax",
        "push ecx",
        "push edx",
        "push dword ptr [esp+24]",
        "push dword ptr [esp+24]",
        "push dword ptr [esp+24]",
        "call {log_fn}",
        "add esp, 12",
        "pop edx",
        "pop ecx",
        "pop eax",
        "jmp [{orig}]",
        log_fn = sym log_strike_fire,
        orig = sym ORIG_STRIKE_FIRE,
    );
}

unsafe extern "cdecl" fn log_strike_fire(worm: u32, subtype_34_ptr: u32, local_struct: u32) {
    let _ = log_line(&format!(
        "[Weapon] StrikeFire: worm=0x{:08X} subtype_34=0x{:08X} local=0x{:08X}",
        worm, subtype_34_ptr, local_struct,
    ));
}

// ── PlacedExplosive (0x51EC80): passthrough (log + call original) ──

#[unsafe(naked)]
unsafe extern "C" fn trampoline_placed_explosive() {
    core::arch::naked_asm!(
        "push eax",
        "push ecx",
        "push edx",
        "push ebx",
        "push esi",
        "push edi",
        "push ebp",
        "push dword ptr [esp+32]",
        "push edx",
        "push ecx",
        "call {log_fn}",
        "add esp, 12",
        "pop ebp",
        "pop edi",
        "pop esi",
        "pop ebx",
        "pop edx",
        "pop ecx",
        "pop eax",
        "jmp [{orig}]",
        log_fn = sym log_placed_explosive,
        orig = sym ORIG_PLACED_EXPLOSIVE,
    );
}

unsafe extern "cdecl" fn log_placed_explosive(local_struct: u32, worm: u32, fire_params: u32) {
    let _ = log_line(&format!(
        "[Weapon] PlacedExplosive: worm=0x{:08X} local=0x{:08X} params=0x{:08X}",
        worm, local_struct, fire_params,
    ));
}

// ── Hook installation ──

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
        hook::install_trap!("FireWeapon", va::FIRE_WEAPON);

        // Full replacements for fire sub-functions
        let _ = hook::install(
            "CreateWeaponProjectile",
            va::CREATE_WEAPON_PROJECTILE,
            hook_create_weapon_projectile as *const (),
        )?;
        let _ = hook::install(
            "ProjectileFire",
            va::PROJECTILE_FIRE,
            hook_projectile_fire as *const (),
        )?;
        let _ = hook::install(
            "CreateArrow",
            va::CREATE_ARROW,
            hook_create_arrow as *const (),
        )?;

        // Passthrough hooks (log + call original)
        let t = hook::install(
            "StrikeFire",
            va::STRIKE_FIRE,
            trampoline_strike_fire as *const (),
        )?;
        ORIG_STRIKE_FIRE.store(t as u32, Ordering::Relaxed);
        let t = hook::install(
            "PlacedExplosive",
            va::PLACED_EXPLOSIVE,
            trampoline_placed_explosive as *const (),
        )?;
        ORIG_PLACED_EXPLOSIVE.store(t as u32, Ordering::Relaxed);
    }

    Ok(())
}
