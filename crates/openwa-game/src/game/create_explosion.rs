//! CreateExplosion — pure-Rust port of 0x00548080.
//!
//! Sends `EntityMessage::Explosion` to the per-game `WorldRootEntity` root, which
//! broadcasts to its children. Every `WorldEntity` child then runs the damage
//! calculation in its own `HandleMessage`.
//!
//! Deviation from WA: the original reserves a 0x408-byte stack buffer (only
//! the first 0x1C are ever populated) and reports that size to HandleMessage.
//! We pass an `ExplosionMessage` directly and report `size_of` — verified
//! that the receiver does not depend on the oversized report.

use openwa_core::fixed::Fixed;

use crate::entity::{BaseEntity, WorldRootEntity};
use crate::game::message::ExplosionMessage;

pub unsafe fn create_explosion(
    pos_x: Fixed,
    pos_y: Fixed,
    sender: *mut BaseEntity,
    explosion_id: u32,
    damage: u32,
    caller_flag: u32,
    owner_id: u32,
) {
    unsafe {
        let world_root = WorldRootEntity::from_shared_data(sender);
        if world_root.is_null() {
            return;
        }
        WorldRootEntity::handle_typed_message_raw(
            world_root,
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
