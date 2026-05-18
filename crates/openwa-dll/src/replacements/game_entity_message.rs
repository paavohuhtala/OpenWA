//! Hooks for `WorldEntity::HandleMessage` (0x004FF280) and its three
//! formerly-bridged helpers. Logic lives in
//! `openwa_game::game::game_entity_message`. Hook installation is generated
//! from `crates/openwa-dll/hooks/entity_message.toml` + `re/**/*.toml`.

use openwa_core::fixed::Fixed;
use openwa_core::vec2::Vec2;
use openwa_game::address::va;
use openwa_game::entity::{BaseEntity, WorldEntity};
use openwa_game::game::{EntityMessage, game_entity_message as gtm};

use crate::hook;

pub(crate) unsafe extern "cdecl" fn cgameentity_handle_message_impl(
    this: *mut WorldEntity,
    sender: *mut BaseEntity,
    msg_type: EntityMessage,
    size: u32,
    data: *mut core::ffi::c_void,
) {
    unsafe {
        gtm::world_entity_handle_message(this, sender, msg_type, size, data as *const u8);
    }
}

pub(crate) unsafe extern "cdecl" fn is_sound_handle_expired_impl(
    this: *mut WorldEntity,
    handle: u32,
) -> u32 {
    unsafe { gtm::sound_handle_expired(this as *const _, handle) }
}

pub(crate) unsafe extern "cdecl" fn compute_explosion_damage_impl(
    this: *mut WorldEntity,
    strength: u32,
    damage: u32,
    pos_x: Fixed,
    pos_y: Fixed,
) -> i32 {
    unsafe { gtm::compute_explosion_damage(this, strength, damage, Vec2::new(pos_x, pos_y)) }
}

pub fn install() -> Result<(), String> {
    unsafe {
        crate::generated::hooks::install_WorldEntity__vt2_HandleMessage()?;
        crate::generated::hooks::install_WorldEntity__IsSoundHandleExpired()?;
        hook::install(
            "WorldEntity::ReleaseSoundHandle",
            va::WORLD_ENTITY_RELEASE_SOUND_HANDLE,
            gtm::release_sound_handle as *const (),
        )?;
        crate::generated::hooks::install_WorldEntity__ComputeExplosionDamage()?;
    }
    Ok(())
}
