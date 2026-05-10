//! Rust port of `MineEntity::Render` (0x00506EF0) and its inline helper
//! `MineEntity::CalcSprite` (0x00506E60).
//!
//! Render emits up to two render-queue commands per frame:
//!   1. The mine's body sprite (always emitted), via `RenderMessage::Sprite`.
//!   2. A countdown / state textbox (only when the per-team text gate
//!      passes), via `RenderMessage::TextboxLocal`.
//!
//! Originally bridged via `bridge_mine_render` in `super::handle_message`.

use core::ffi::c_char;
use core::fmt::Write as _;

use heapless::String as HString;

use super::MineEntity;
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::rebase::rb;
use crate::render::message::RenderMessage;
use crate::render::sprite::sprite_op::SpriteOp;
use crate::render::textbox::set_text as set_textbox_text;
use openwa_core::fixed::Fixed;

// ─── Bridges ───────────────────────────────────────────────────────────────

static mut DROWN_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        DROWN_ADDR = rb(0x00565D60);
    }
}

/// `drown` (0x00565D60) — fastcall(ECX = sprite), plain RET. Pure
/// substitution table: maps an in-air sprite ID (low 16 bits) to its
/// underwater counterpart, preserving the high 16 bits.
unsafe fn drown(sprite: u32) -> u32 {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32) -> u32 = core::mem::transmute(DROWN_ADDR as usize);
        f(sprite)
    }
}

// ─── Static text addresses (in WA's .rdata) ────────────────────────────────

/// `"Dud"` at WA 0x006643D8.
const DUD_TEXT_VA: u32 = 0x006643D8;
/// `"?"` at WA 0x00661654.
const QUESTION_TEXT_VA: u32 = 0x00661654;

// ─── CalcSprite ────────────────────────────────────────────────────────────

/// Rust port of `MineEntity::CalcSprite` (0x00506E60). Returns
/// `(sprite_id, sub_frame_angle)`. The angle is interpolated from the
/// mine's stored angle plus its per-frame angular accumulator scaled by
/// [`GameWorld::render_interp_a`].
unsafe fn calc_sprite(this: *mut MineEntity) -> (u32, Fixed) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        // Sub-frame angle = _field_98 * render_interp_a + angle (all Fixed).
        let mut sub_frame_angle =
            (*this).base._field_98.mul_raw((*world).render_interp_a) + (*this).base.angle;

        // Default sprite is 0x2F. When triggered and `(fuse_timer * 4 / 1000)
        // & 1 != 0`, swap to the flashing 0x2D.
        let triggered = (*this).triggered_flag != 0;
        let mut sprite = if triggered {
            let fuse = (*this).fuse_timer;
            let v = ((fuse as i64 * 4) / 1000) as i32;
            if v & 1 != 0 { 0x2D } else { 0x2F }
        } else {
            0x2F
        };

        // Drown gate — underwater (`_field_b0`) or wet (`_field_a4`) swaps
        // to the underwater sprite via WA's lookup table.
        let underwater = (*this).base._field_b0 != 0;
        let wet = (*this).base._field_a4 != 0;
        if underwater || wet {
            sprite = drown(sprite);
        }
        // Fully underwater forces sub-frame angle to 0 — anchors the
        // underwater sprite at the mine's screen y rather than offsetting
        // by the rotation term.
        if underwater {
            sub_frame_angle = Fixed::ZERO;
        }

        (sprite, sub_frame_angle)
    }
}

// ─── Render ────────────────────────────────────────────────────────────────

/// Render-time anchor offset (18 pixels) used by both the body-sprite
/// layer math and the textbox y-anchor.
const MINE_TEXTBOX_Y_OFFSET: Fixed = Fixed::from_raw(0x00120000);

/// Rust port of `MineEntity::Render` (0x00506EF0). stdcall(this), RET 0x4.
pub unsafe fn mine_render(this: *mut MineEntity) {
    unsafe {
        let (sprite_id, sub_frame_angle) = calc_sprite(this);
        let world = (*(this as *const BaseEntity)).world;
        let pos_x = (*this).base.pos.x;
        let pos_y = (*this).base.pos.y;
        let triggered = (*this).triggered_flag != 0;
        let activity_rank = (*this).activity_rank_slot as i32;

        let primary_render_rank = pick_render_rank(world, activity_rank);

        // Emit the body sprite.
        // WA: `EAX = (triggered ? 0xFFF80000 : 0) + 0x120000`. The triggered
        // branch overflows: `0xFFF80000 + 0x120000 = 0x000A0000` (mod 2^32).
        let sprite_op_flags: u32 = if triggered { 0x000A0000 } else { 0x00120000 };
        let layer = sprite_op_flags
            .wrapping_add((primary_render_rank as u32).wrapping_mul(2))
            .wrapping_add(1);
        let rq = (*world).render_queue;
        let _ = (*rq).push_typed(
            layer,
            RenderMessage::Sprite {
                local: true,
                x: pos_x.floor(),
                y: pos_y.floor(),
                sprite: SpriteOp(sprite_id),
                anim_value: sub_frame_angle,
            },
        );

        // Textbox-render gate: three world/game_info conditions must hold.
        let textbox_threshold = 3i32 - if (*world).terrain_pct_b != 0 { 1 } else { 0 };
        if ((*world)._field_7640 as i32) >= textbox_threshold {
            return;
        }
        if (*world)._field_7648 == 0 {
            return;
        }
        let game_info = (*world).game_info;
        // Mine countdown textbox shows only during replay playback —
        // gated on `replay_flag_a` (low byte of `replay_flags_packed`)
        // specifically, not the whole u32.
        if ((*game_info).replay_flags_packed as u8) == 0 {
            return;
        }

        // Textbox reuses the same render-rank lookup. Recompute (WA rereads
        // it to avoid spilling the value across the gate).
        let textbox_render_rank = pick_render_rank(world, activity_rank);

        // Pick the displayed text.
        let mut text_buf: HString<16> = HString::new();
        let text_ptr = pick_textbox_text(this, world, &mut text_buf);

        // SetTextboxText layout + blit. The mine textbox is filled with
        // `gfx_color_table[7]` and bordered with `gfx_color_table[6]`.
        let mut text_w: i32 = 0;
        let mut text_h: i32 = 0;
        let textbox = (*this).textbox_handle;
        let fill_color = (*world).gfx_color_table[7];
        let border_color = (*world).gfx_color_table[6];
        let bitmap = set_textbox_text(
            textbox,
            text_ptr,
            7,
            fill_color,
            border_color,
            &mut text_w,
            &mut text_h,
            Fixed::ONE,
        );

        // RQ_DrawTextboxLocal — anchored 18 pixels above the mine's pos_y.
        let textbox_layer = (textbox_render_rank as u32)
            .wrapping_mul(2)
            .wrapping_add(0xD0200);
        let textbox_y = pos_y - MINE_TEXTBOX_Y_OFFSET;
        let _ = (*rq).push_typed(
            textbox_layer,
            RenderMessage::TextboxLocal {
                x: pos_x.floor(),
                y: textbox_y.floor(),
                bitmap,
                src_w: text_w,
                src_h: text_h,
                flags: 0,
            },
        );
    }
}

/// Pick the activity-queue render rank for a mine — the value used for
/// sprite layer ordering so older mines render in front of newer ones.
/// When `activity_rank_slot < 0` (mine has no slot yet — unplaced or
/// anonymous pre-placed), the lookup falls back to the queue's `capacity`
/// (when `> 0x100`) or `count`; otherwise it returns
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

/// Pick the text shown in the mine's countdown textbox.
///
/// Branches:
/// - `fled != 0` → static `"Dud"` from WA's .rdata.
/// - Positive `fuse_timer` AND not in `?`-mode → `"%d"` of `fuse / 1000`.
/// - Negative `fuse_timer` AND replay-recorded fuse available → `"%d.%02d"`
///   of the recorded fuse, converted to centisecond pairs.
/// - Otherwise (no recorded fuse and still negative) → static `"?"`.
unsafe fn pick_textbox_text(
    this: *mut MineEntity,
    world: *const GameWorld,
    out_buf: &mut HString<16>,
) -> *const c_char {
    unsafe {
        if (*this).fled != 0 {
            return rb(DUD_TEXT_VA) as *const c_char;
        }

        let fuse = (*this).fuse_timer;
        let game_info = (*world).game_info;
        let scheme_uses_recorded_fuse = (*this).init_fuse_ms >= 0
            && (*game_info).mine_textbox_mode >= 0
            && (*this).triggered_flag == 0;

        if fuse >= 0 && scheme_uses_recorded_fuse {
            // Plain seconds — `fuse / 1000`.
            let _ = write!(out_buf, "{}\0", fuse / 1000);
            return out_buf.as_ptr() as *const c_char;
        }

        // Either fuse is negative OR scheme path falls through. Try the
        // replay-recorded fuse before giving up to "?".
        let mut effective_fuse = fuse;
        if effective_fuse < 0 {
            let track_idx = (*this)._field_194;
            let log_ptr = (*world)._unknown_51c;
            if track_idx != 0xFFFFFFFF
                && !log_ptr.is_null()
                && let Some(recorded) = log_lookup(log_ptr, track_idx)
            {
                effective_fuse = recorded;
            }
        }

        if effective_fuse < 0 {
            return rb(QUESTION_TEXT_VA) as *const c_char;
        }

        // "%d.%02d" of `((fuse+19)/20)*2` as centisecond-ish pairs.
        let units = ((effective_fuse.wrapping_add(0x13)) / 0x14).wrapping_mul(2);
        let secs = units / 100;
        let cents = units % 100;
        let _ = write!(out_buf, "{}.{:02}\0", secs, cents);
        out_buf.as_ptr() as *const c_char
    }
}

/// Read `replay_log.recorded_fuses[track_idx]` with the same
/// bounds-check WA's inline `_Vector_at` (FUN_00507c20) performs. The
/// vector lives at `replay_log + 0x18` (`begin`/`end` ptr pair).
/// Returns `None` if the index is out of range.
unsafe fn log_lookup(log_ptr: *mut u8, track_idx: u32) -> Option<i32> {
    unsafe {
        let vec_base = log_ptr.add(0x18) as *const u8;
        let begin = *(vec_base.add(0x4) as *const *const i32);
        let end = *(vec_base.add(0x8) as *const *const i32);
        if begin.is_null() {
            return None;
        }
        let len = (end as usize - begin as usize) / 4;
        if (track_idx as usize) >= len {
            return None;
        }
        Some(*begin.add(track_idx as usize))
    }
}
