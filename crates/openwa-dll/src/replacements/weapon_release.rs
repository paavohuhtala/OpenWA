//! WeaponRelease (0x0051C3D0) and SpawnEffect (0x00547C30) hooks.
//!
//! Thin hook shim — game logic lives in `openwa_game::game::weapon_release`.
//! Hook installation is generated from `crates/openwa-dll/hooks/weapon.toml`
//! + `re/**/*.toml`.

use openwa_core::fixed::Fixed;
use openwa_game::entity::BaseEntity;
use openwa_game::entity::worm::WormEntity;
use openwa_game::game::weapon_release as wr;

pub(crate) unsafe extern "cdecl" fn weapon_release_impl(
    worm: *mut WormEntity,
    spawn_x: u32,
    spawn_y: u32,
    aim_dir_x: Fixed,
    aim_dir_y: Fixed,
) {
    unsafe {
        wr::weapon_release(worm, spawn_x, spawn_y, aim_dir_x, aim_dir_y);
    }
}

pub(crate) unsafe extern "cdecl" fn spawn_effect_cdecl(
    sender: *mut BaseEntity,
    constant: u32,
    speed_x: Fixed,
    speed_y: Fixed,
    rng_scaled: i32,
    rng_offset: i32,
    palette: u32,
    state_flag: u32,
    size: Fixed,
    scale: Fixed,
) {
    unsafe {
        wr::spawn_effect(
            sender, constant, speed_x, speed_y, rng_scaled, rng_offset, palette, state_flag, size,
            scale,
        );
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        crate::generated::hooks::install_SpawnEffect()?;
        crate::generated::hooks::install_WeaponRelease()?;
    }
    Ok(())
}
