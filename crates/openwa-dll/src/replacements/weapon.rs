//! Weapon hooks (ammo + fire pipeline).
//!
//! Thin hook shim — all game logic lives in `openwa_game::game::weapon_fire`
//! and `openwa_game::game::weapon_aim_flags`. All hooks are codegen-driven
//! via `hooks/weapon.toml` + `re/**/*.toml`.

use openwa_game::address::va;
use openwa_game::engine::TeamArena;
use openwa_game::entity::worm::WormEntity;
use openwa_game::game::weapon::{WeaponFireParams, WeaponReleaseContext};
use openwa_game::game::weapon_fire;

use crate::generated::hooks;
use crate::hook;

// ── TeamArena__AddAmmo (0x522640) ──

pub(crate) unsafe extern "cdecl" fn add_ammo_impl(
    team_index: u32,
    amount: i32,
    arena: *mut TeamArena,
    weapon_id: u32,
) {
    unsafe {
        weapon_fire::add_ammo(team_index, amount, arena, weapon_id);
    }
}

// ── TeamArena__SubtractAmmo (0x522680) ──

pub(crate) unsafe extern "cdecl" fn subtract_ammo_impl(
    team_index: u32,
    arena: *mut TeamArena,
    weapon_id: u32,
) {
    unsafe {
        weapon_fire::subtract_ammo(team_index, arena, weapon_id);
    }
}

// ── TeamArena__GetAmmo (0x5225E0) ──

pub(crate) unsafe extern "cdecl" fn get_ammo_impl(
    team_index: u32,
    arena: *mut TeamArena,
    weapon_id: u32,
) -> u32 {
    unsafe { weapon_fire::get_ammo(team_index, arena, weapon_id) }
}

// ── TeamArena__CountAliveWorms (0x5225A0) ──

pub(crate) unsafe extern "cdecl" fn count_alive_worms_impl(
    team_index: u32,
    arena: *mut TeamArena,
) -> u32 {
    unsafe { weapon_fire::count_alive_worms(team_index, arena) }
}

// ── FireWeapon__CreateWeaponProjectile (0x51E0F0) ──
// Thiscall with `this: WormEntity *` typed in TOML → custom_storage =
// true, so the impl is `extern "cdecl"` (trampoline bridges).

pub(crate) unsafe extern "cdecl" fn hook_create_weapon_projectile(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    spawn_data: *const WeaponReleaseContext,
) {
    unsafe {
        weapon_fire::create_weapon_projectile(worm, fire_params, spawn_data);
    }
}

// ── FireWeapon__ProjectileFire (0x51DFB0): default-storage stdcall ──

pub(crate) unsafe extern "stdcall" fn hook_projectile_fire(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    release_ctx: *const WeaponReleaseContext,
) {
    unsafe {
        weapon_fire::projectile_fire(worm, fire_params, release_ctx);
    }
}

// ── FireWeapon__CreateArrow (0x51ED90) ──
// Same thiscall + typed-this shape as CreateWeaponProjectile.

pub(crate) unsafe extern "cdecl" fn hook_create_arrow(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    release_ctx: *const WeaponReleaseContext,
) {
    unsafe {
        weapon_fire::create_arrow(worm, fire_params, release_ctx);
    }
}

// ── WeaponSpawn__DecodeDescriptor (0x00565C10) ──
// Generated installer points directly at the openwa-game impl.

// ── Hook installation ──

pub fn install() -> Result<(), String> {
    unsafe {
        hooks::install_TeamArena__AddAmmo()?;
        hooks::install_TeamArena__GetAmmo()?;
        hooks::install_TeamArena__SubtractAmmo()?;
        hooks::install_TeamArena__CountAliveWorms()?;

        // FireWeapon (0x51EE60) has no remaining callers — every WA path
        // into it now goes through ported Rust.
        hook::install_trap!("FireWeapon", va::FIRE_WEAPON);

        hooks::install_FireWeapon__CreateWeaponProjectile()?;
        hooks::install_FireWeapon__ProjectileFire()?;
        hooks::install_FireWeapon__CreateArrow()?;

        hooks::install_WeaponSpawn__DecodeDescriptor()?;
    }

    Ok(())
}
