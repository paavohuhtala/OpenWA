//! WorldEntity::TryMovePosition (0x004FE070) full replacement.
//!
//! Generic position-update primitive used by 50+ entity subclasses
//! (constructors and movement code). Game logic lives in
//! [`openwa_game::entity::WorldEntity::try_move_position_raw`].
//!
//! Calling convention: `__usercall(ESI=this, EDI=y, [ESP+4]=x)`, RET 0x4.

use openwa_core::fixed::Fixed;
use openwa_game::entity::WorldEntity;
use openwa_game::entity::game_entity;

pub(crate) unsafe extern "cdecl" fn try_move_position_impl(
    this: *mut WorldEntity,
    y: Fixed,
    x: Fixed,
) -> u32 {
    unsafe { WorldEntity::try_move_position_raw(this, x, y) }
}

pub fn install() -> Result<(), String> {
    unsafe {
        // `try_move_position_raw` (and its only caller of substance,
        // `check_move_collision_raw`) need the
        // `CollisionManager::update_intersections` bridge address.
        // Same call also primes the `WorldEntity::Constructor` bridge
        // used by every typed subclass ctor (mine, oil drum, …).
        game_entity::init_addrs();

        crate::generated::hooks::install_WorldEntity__TryMovePosition()?;
    }
    Ok(())
}
