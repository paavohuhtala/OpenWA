//! CreateExplosion — pure-Rust port of 0x00548080.
//!
//! Deviation from WA: the original reserves a 0x408-byte stack buffer (only
//! the first 0x1C are ever populated) and reports that size to HandleMessage.
//! We pass an `ExplosionMessage` directly and report `size_of` — verified
//! that `CTaskCross::HandleMessage` does not depend on the oversized report.

use openwa_core::fixed::Fixed;

use crate::game::message::TaskMessage;
use crate::task::{CTask, SharedDataTable};
use core::ffi::c_void;

/// SharedData key for the explosion manager (CTaskCross).
const EXPLOSION_MANAGER_KEY_EDI: u32 = 0x14;

/// Payload for `TaskMessage::Explosion` (0x1c), consumed by
/// `CTaskCross::HandleMessage` (vtable slot 2).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ExplosionMessage {
    /// Always `1` from WA's `CreateExplosion`. Role on the receiver side
    /// unconfirmed — likely a "real vs. cosmetic" discriminator, since
    /// `SpawnEffect` populates the matching slot with its own constant.
    pub flag: u32,
    pub pos_x: Fixed,
    pub pos_y: Fixed,
    pub explosion_id: u32,
    pub damage: u32,
    /// Caller-supplied flag of unknown purpose. Missile contact passes 0,
    /// but other WA call sites pass non-zero values — asserted empirically.
    pub caller_flag: u32,
    pub owner_id: u32,
}

const _: () = assert!(core::mem::size_of::<ExplosionMessage>() == 0x1C);

pub unsafe fn create_explosion(
    pos_x: Fixed,
    pos_y: Fixed,
    sender: *mut CTask,
    explosion_id: u32,
    damage: u32,
    caller_flag: u32,
    owner_id: u32,
) {
    unsafe {
        let msg = ExplosionMessage {
            flag: 1,
            pos_x,
            pos_y,
            explosion_id,
            damage,
            caller_flag,
            owner_id,
        };
        dispatch(sender, &msg);
    }
}

unsafe fn dispatch(sender: *mut CTask, msg: &ExplosionMessage) {
    unsafe {
        let table = SharedDataTable::from_task(sender as *const CTask);
        let entity = table.lookup(0, EXPLOSION_MANAGER_KEY_EDI);
        if entity.is_null() {
            return;
        }

        let vtable = *(entity as *const *const usize);
        let handle_msg: unsafe extern "thiscall" fn(*mut u8, *mut c_void, u32, u32, *const u8) =
            core::mem::transmute(*vtable.add(2));
        handle_msg(
            entity,
            sender as *mut c_void,
            TaskMessage::Explosion as u32,
            core::mem::size_of::<ExplosionMessage>() as u32,
            msg as *const ExplosionMessage as *const u8,
        );
    }
}
