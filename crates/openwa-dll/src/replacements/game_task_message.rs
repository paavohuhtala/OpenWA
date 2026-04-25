//! `WorldEntity::HandleMessage` hook (0x004FF280, vtable slot 2).
//!
//! Thin hook shim — logic lives in
//! `openwa_game::game::game_task_message::cgametask_handle_message`.

use openwa_game::address::va;
use openwa_game::game::{TaskMessage, game_task_message as gtm};
use openwa_game::task::{BaseEntity, WorldEntity};

use crate::hook::{self, usercall_trampoline};

// thiscall(this, sender, msg_type, size, data) + 4 stack params, RET 0x10.
usercall_trampoline!(fn trampoline_cgametask_handle_message;
    impl_fn = cgametask_handle_message_impl;
    reg = ecx; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn cgametask_handle_message_impl(
    this: *mut WorldEntity,
    sender: *mut BaseEntity,
    msg_type: TaskMessage,
    size: u32,
    data: *const u8,
) {
    unsafe {
        gtm::cgametask_handle_message(this, sender, msg_type, size, data);
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "WorldEntity::HandleMessage",
            va::CGAMETASK_VT2_HANDLE_MESSAGE,
            trampoline_cgametask_handle_message as *const (),
        )?;
    }
    Ok(())
}
