//! WorldEntity::TryMovePosition (0x004FE070) full replacement.
//!
//! Generic position-update primitive used by 50+ entity subclasses
//! (constructors and movement code). Game logic lives in
//! [`openwa_game::task::WorldEntity::try_move_position_raw`].
//!
//! Calling convention: `__usercall(ESI=this, EDI=y, [ESP+4]=x)`, RET 0x4.

use crate::hook::{self, usercall_trampoline};
use openwa_game::address::va;
use openwa_game::task::WorldEntity;

unsafe extern "cdecl" fn try_move_position_impl(this: *mut WorldEntity, y: i32, x: i32) -> u32 {
    unsafe { WorldEntity::try_move_position_raw(this, x, y) }
}

usercall_trampoline!(fn trampoline_try_move_position; impl_fn = try_move_position_impl;
    regs = [esi, edi]; stack_params = 1; ret_bytes = "0x4");

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install(
            "WorldEntity::TryMovePosition",
            va::TRY_MOVE_POSITION,
            trampoline_try_move_position as *const (),
        )?;
    }
    Ok(())
}
