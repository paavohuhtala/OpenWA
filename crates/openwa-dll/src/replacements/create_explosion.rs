//! CreateExplosion hook (0x00548080).
//!
//! Thin hook shim — game logic lives in `openwa_game::game::create_explosion`.
//! Hook installation is generated from `crates/openwa-dll/hooks/explosion.toml`
//! + `re/**/*.toml`.

use openwa_core::fixed::Fixed;
use openwa_game::entity::BaseEntity;
use openwa_game::game::create_explosion as ce;

pub(crate) unsafe extern "cdecl" fn create_explosion_cdecl(
    pos_x: Fixed,
    pos_y: Fixed,
    sender: *mut BaseEntity,
    explosion_id: u32,
    damage: u32,
    caller_flag: u32,
    owner_id: u32,
) {
    unsafe {
        ce::create_explosion(
            pos_x,
            pos_y,
            sender,
            explosion_id,
            damage,
            caller_flag,
            owner_id,
        );
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        crate::generated::hooks::install_CreateExplosion()?;
    }
    Ok(())
}
