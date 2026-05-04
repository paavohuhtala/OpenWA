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

use openwa_game::entity::worm::WormEntity;
use openwa_game::game::weapon::WeaponFireParams;
use openwa_game::game::weapon_aim_flags;
use openwa_game::game::weapon_fire;
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

// ── CreateWeaponProjectile (0x51E0F0): thiscall(ECX=worm, fire_params, local_struct) ──

unsafe extern "thiscall" fn hook_create_weapon_projectile(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const u8,
) {
    unsafe {
        weapon_fire::create_weapon_projectile(worm, fire_params, local_struct);
    }
}

// ── ProjectileFire (0x51DFB0): stdcall(worm, fire_params, local_struct) ──

unsafe extern "stdcall" fn hook_projectile_fire(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const openwa_game::game::weapon::WeaponSpawnData,
) {
    unsafe {
        weapon_fire::projectile_fire(worm, fire_params, local_struct);
    }
}

// ── CreateArrow (0x51ED90): thiscall(ECX=worm, fire_params, local_struct) ──

unsafe extern "thiscall" fn hook_create_arrow(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const u8,
) {
    unsafe {
        weapon_fire::create_arrow(worm, fire_params, local_struct);
    }
}

// ── WeaponSpawn::DecodeDescriptor (0x00565C10) ──
// usercall(EAX = out_a, EDX = out_b) + 7 stack args (entry, out_c..out_h),
// RET 0x1C. The Rust impl [`weapon_aim_flags::decode_weapon_aim_flags_impl`]
// has a matching cdecl signature; the trampoline forwards EAX/EDX as the
// first two cdecl args and the 7 stack args fall through unchanged.

usercall_trampoline!(fn trampoline_decode_weapon_aim_flags;
    impl_fn = weapon_aim_flags::decode_weapon_aim_flags_impl;
    regs = [eax, edx]; stack_params = 7; ret_bytes = "0x1c");

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

        let _ = hook::install(
            "WeaponSpawn__DecodeDescriptor",
            va::WEAPON_SPAWN_DECODE_DESCRIPTOR,
            trampoline_decode_weapon_aim_flags as *const (),
        )?;
    }

    Ok(())
}
