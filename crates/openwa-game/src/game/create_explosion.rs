//! CreateExplosion — pure-Rust port of 0x00548080.
//!
//! Sends `TaskMessage::Explosion` to the per-game `CTaskTurnGame` root, which
//! broadcasts to its children. Every `CGameTask` child then runs the damage
//! calculation in its own `HandleMessage`.
//!
//! Deviation from WA: the original reserves a 0x408-byte stack buffer (only
//! the first 0x1C are ever populated) and reports that size to HandleMessage.
//! We pass an `ExplosionMessage` directly and report `size_of` — verified
//! that the receiver does not depend on the oversized report.

use openwa_core::fixed::Fixed;

use crate::game::message::ExplosionMessage;
use crate::task::{CTask, CTaskTurnGame};

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
        let turn_game = CTaskTurnGame::from_shared_data(sender);
        if turn_game.is_null() {
            return;
        }
        CTaskTurnGame::handle_typed_message_raw(
            turn_game,
            sender,
            ExplosionMessage {
                flag: 1,
                pos_x,
                pos_y,
                explosion_id,
                damage,
                caller_flag,
                owner_id,
            },
        );
    }
}
