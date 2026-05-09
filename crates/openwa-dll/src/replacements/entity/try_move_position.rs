//! WorldEntity::TryMovePosition (0x004FE070) full replacement.
//!
//! Generic position-update primitive used by 50+ entity subclasses
//! (constructors and movement code). Game logic lives in
//! [`openwa_game::entity::WorldEntity::try_move_position_raw`].
//!
//! Calling convention: `__usercall(ESI=this, EDI=y, [ESP+4]=x)`, RET 0x4.

use crate::hook::{self, usercall_trampoline};
use openwa_core::fixed::Fixed;
use openwa_game::address::va;
use openwa_game::entity::WorldEntity;
use openwa_game::entity::game_entity;

unsafe extern "cdecl" fn try_move_position_impl(this: *mut WorldEntity, y: i32, x: i32) -> u32 {
    unsafe { WorldEntity::try_move_position_raw(this, Fixed(x), Fixed(y)) }
}

usercall_trampoline!(fn trampoline_try_move_position; impl_fn = try_move_position_impl;
    regs = [esi, edi]; stack_params = 1; ret_bytes = "0x4");

pub fn install() -> Result<(), String> {
    unsafe {
        // `try_move_position_raw` (and its only caller of substance,
        // `check_move_collision_raw`) need the
        // `CollisionManager::update_intersections` bridge address.
        game_entity::init_addrs();

        let _ = hook::install(
            "WorldEntity::TryMovePosition",
            va::TRY_MOVE_POSITION,
            trampoline_try_move_position as *const (),
        )?;
    }
    Ok(())
}
