//! `WormEntity::DrawTurnIndicator` (0x0050F810). One sprite per frame
//! per worm — a small icon over the worm whose turn it is, with a smooth
//! fade-in / fade-out driven by `aim_fade[2]` easing toward `aim_fade[3]`.

use crate::entity::WormEntity;
use crate::entity::base::BaseEntity;
use crate::render::message::RenderMessage;
use crate::render::sprite::sprite_op::SpriteOp;
use openwa_core::fixed::Fixed;

/// `aim_fade[2]` slew rate (5% of remaining gap per sim tick) and the
/// constant-step floor that prevents asymptotic creep — same values used
/// by [`crate::entity::worm_handle_message::ease_aim_vec_b`] when the
/// real `aim_fade[2]` write happens during `EaseAimVecB`.
const AIM_FADE_RATE: Fixed = Fixed(0xCCC);
const AIM_FADE_MIN_STEP: Fixed = Fixed(0x20C);

/// Rust port of `WormEntity::DrawTurnIndicator` (0x0050F810).
/// thiscall(this), plain RET.
///
/// Computes a transient "interpolated `aim_fade[2]`" by reproducing
/// [`Fixed::smooth_move_towards`]'s step (without mutating the real
/// field), scales it by `world.render_interp_b` for sub-tick smoothness,
/// then folds it into the y-mul factor. The result is a Fixed-shaped
/// value passed as the legacy `DrawSpriteLocal`'s `y` argument — the
/// dispatcher interprets it as a screen-space y when the sprite is local.
pub unsafe fn draw_turn_indicator(this: *mut WormEntity) {
    unsafe {
        let current = (*this).aim_fade[2];
        if current == Fixed::ZERO {
            return;
        }
        let target = (*this).aim_fade[3];

        // Reproduce `smooth_move_towards`'s step without mutating the
        // real field — DrawTurnIndicator only consumes the eased value
        // for this frame's render; the actual `aim_fade[2]` write
        // happens once per sim tick in `EaseAimVecB`.
        let mut tentative = current;
        tentative.smooth_move_towards(target, AIM_FADE_MIN_STEP, AIM_FADE_RATE);
        let step = Fixed(tentative.0.wrapping_sub(current.0));

        let world = (*(this as *const BaseEntity)).world;

        // Render-interp scaling: step * world.render_interp_b (Fixed mul).
        let interp_b = (*world).render_interp_b;
        let scaled_step_raw = ((step.0 as i64).wrapping_mul(interp_b.0 as i64) >> 16) as i32;
        let interp_value = current.0.wrapping_add(scaled_step_raw);

        // y_mul = pos_y + 384.0 (Fixed integer +0x180)
        // y_pos = (y_mul * interp_value) >> 16 - 464.0 (Fixed integer -0x1D0)
        let pos_y_raw = (*this).base.pos_y.0;
        let y_mul = pos_y_raw.wrapping_add(0x01800000);
        let y_pos_raw = (((y_mul as i64).wrapping_mul(interp_value as i64) >> 16) as i32)
            .wrapping_sub(0x01D00000);
        let y_pos = Fixed(y_pos_raw);

        // sprite = (world._field_5ec != 0 ? 6 : 0) + 0x14 + this._unknown_10c
        let sprite_base: u32 = if (*world)._field_5ec != 0 { 6 } else { 0 };
        let sprite_id = sprite_base
            .wrapping_add(0x14)
            .wrapping_add((*this)._unknown_10c);

        // anim_value = world.frame / 50 — frame counter scaled into a
        // Fixed seconds-elapsed value.
        let anim_value = Fixed::from_raw((((*world).frame as i32) << 16).wrapping_div(50));

        let pos_x = (*this).base.pos_x;
        let rq = (*world).render_queue;
        let _ = (*rq).push_typed(
            0x30000,
            RenderMessage::Sprite {
                local: true,
                x: pos_x.floor(),
                y: y_pos.floor(),
                sprite: SpriteOp(sprite_id),
                anim_value,
            },
        );
    }
}
