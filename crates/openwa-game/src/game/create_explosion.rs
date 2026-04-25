//! CreateExplosion — pure-Rust port of 0x00548080.
//!
//! Sends `TaskMessage::Explosion` to the per-game `CTaskTurnGame` root, which
//! broadcasts to its children. Every `CGameTask` child then runs the damage
//! calculation in its own `HandleMessage`.
//!
//! Deviation from WA: the original reserves a 0x408-byte stack buffer (only
//! the first 0x1C are ever populated) and reports that size to HandleMessage.
//! We pass an `ExplosionMessage` directly and report `size_of` — verified that
//! the receiver does not depend on the oversized report.

use openwa_core::fixed::Fixed;

use crate::game::message::TaskMessage;
use crate::task::{CTask, CTaskTurnGame};

/// Payload for `TaskMessage::Explosion` (0x1c), consumed by every
/// `CGameTask::HandleMessage` reached through the broadcast.
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
        let turn_game = CTaskTurnGame::from_shared_data(sender);
        if turn_game.is_null() {
            return;
        }
        CTaskTurnGame::handle_message_raw(
            turn_game,
            sender,
            TaskMessage::Explosion as u32,
            core::mem::size_of::<ExplosionMessage>() as u32,
            msg as *const ExplosionMessage as *const u8,
        );
    }
}
