//! Rust port of `Task_OilDrum::render` (0x00505000).
//!
//! Emits one render-queue command per frame: the drum's body sprite at
//! `0x6E + damage_step` (0..=3, scaling from intact to nearly-detonated),
//! optionally substituted via the underwater `drown` lookup table when
//! the drum is submerged.
//!
//! Originally bridged via `bridge_oil_drum_render` in
//! `super::handle_message`.

use core::sync::atomic::{AtomicU32, Ordering};

use super::OilDrumEntity;
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::rebase::rb;
use crate::render::message::RenderMessage;
use crate::render::sprite::sprite_op::SpriteOp;

// ─── Bridges ───────────────────────────────────────────────────────────────

static DROWN_ADDR: AtomicU32 = AtomicU32::new(0);

pub unsafe fn init_addrs() {
    DROWN_ADDR.store(rb(0x00565D60), Ordering::Relaxed);
}

/// `drown` (0x00565D60) — fastcall(ECX = sprite), plain RET. Pure
/// substitution table: maps an in-air sprite ID (low 16 bits) to its
/// underwater counterpart, preserving the high 16 bits. Same helper the
/// mine renderer uses.
unsafe fn drown(sprite: u32) -> u32 {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32) -> u32 =
            core::mem::transmute(DROWN_ADDR.load(Ordering::Relaxed) as usize);
        f(sprite)
    }
}

// ─── Render ────────────────────────────────────────────────────────────────

/// Rust port of `Task_OilDrum::render` (0x00505000). usercall(EAX = this),
/// plain RET.
pub unsafe fn oil_drum_render(this: *mut OilDrumEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;

        let render_rank = pick_render_rank(world, (*this).activity_rank_slot);

        // Damage-step sprite ladder. Signed division by `max_health`
        // mirrors WA (`CDQ; IDIV [ESI+0x108]`); cap at 3.
        let mut damage_idx = (*this).damage_received.wrapping_mul(4) / (*this).max_health;
        if damage_idx > 3 {
            damage_idx = 3;
        }
        let mut sprite_id = (damage_idx as u32).wrapping_add(0x6E);

        // Underwater swap.
        if (*this).base._field_b0 != 0 {
            sprite_id = drown(sprite_id);
        }

        // Animation phase from world frame counter:
        //   palette = (world.frame << 16) / 50  (signed div).
        // WA materialises the result via the 0x51EB851F magic constant —
        // an ordinary signed divide is byte-identical.
        let frame = (*world).frame as i32;
        let palette = ((frame as i64).wrapping_shl(16) / 50) as u32;

        let layer = (render_rank as u32).wrapping_add(0x100001);
        let rq = (*world).render_queue;
        let _ = (*rq).push_typed(
            layer,
            RenderMessage::Sprite {
                local: true,
                x: pos_x.floor(),
                y: pos_y.floor(),
                sprite: SpriteOp(sprite_id),
                palette,
            },
        );
    }
}

/// Pick the activity-queue render rank for an oil drum. Identical to the
/// mine's helper: when the drum's slot is `< 0` (queue was full at
/// construction time, no slot acquired), fall back to the queue's
/// capacity (when `> 0x100`) or its current count; otherwise return
/// `entity_activity_queue.ages[slot]`.
unsafe fn pick_render_rank(world: *const GameWorld, activity_rank_slot: i32) -> i32 {
    unsafe {
        let queue = &(*world).entity_activity_queue;
        if activity_rank_slot < 0 {
            let capacity = queue.capacity as i32;
            if capacity > 0x100 {
                capacity
            } else {
                queue.count as i32
            }
        } else {
            queue.ages[activity_rank_slot as usize] as i32
        }
    }
}
