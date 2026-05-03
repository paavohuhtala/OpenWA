//! Crosshair line rendering — draws the weapon aiming crosshair line.
//!
//! Ported from WA.exe `DrawCrosshairLine` (0x5197D0).
//! Draws:
//!   1. Compute direction from angle at entity+0x264
//!   2. Compute line length from GameWorld scale + entity offset
//!   3. Endpoint = start + direction * length (with overflow clamping)
//!   4. DrawPolygon (2 vertices) for the line
//!   5. Conditionally DrawSpriteLocal at endpoint (crosshair sprite)

use crate::entity::WeaponAimEntity;
use crate::render::SpriteOp;
use crate::render::message::RenderMessage;
use openwa_core::fixed::Fixed;
use openwa_core::trig::{cos, sin};

/// Draw the weapon aiming crosshair line for the given entity.
///
/// # Safety
///
/// `entity_ptr` must point to a valid `WeaponAimEntity`. ASLR rebase must be
/// initialized.
pub unsafe fn draw_crosshair_line(entity: *const WeaponAimEntity) {
    unsafe {
        let gt = &(*entity).game_entity;

        if (*entity).aim_active == 0 {
            return;
        }

        let world = &*gt.base.world;
        let rq = &mut *world.render_queue;

        let start_x = gt.pos_x.0;
        let start_y = gt.pos_y.0;

        let angle = (*entity).aim_angle;

        // Trig interpolation
        let sin_interp = sin(angle);
        let cos_interp = cos(angle);

        // Smooth aim-range animation: interpolate 20 units/tick by the sub-frame
        // progress ratio, then add the crosshair's standing aim offset.
        let scale = world.render_interp_a.mul_raw(Fixed(0x14_0000)).0 + (*entity).aim_range_offset;

        // Endpoint = start + direction * scale
        let mut endpoint_x = Fixed(sin_interp.0)
            .mul_raw(Fixed(scale))
            .0
            .wrapping_add(start_x);
        let mut endpoint_y = Fixed(cos_interp.0)
            .mul_raw(Fixed(scale))
            .0
            .wrapping_add(start_y);

        // Overflow clamping — when endpoint overflows i32 due to large scale
        let mut overflowed = false;
        let mut clamp_factor = 0i32;

        let threshold = (*world.game_info).game_version;

        if threshold > 0x11E {
            // Check X overflow: sin > 0 but endpoint wrapped below start
            if sin_interp.0 > 0 && endpoint_x < start_x {
                overflowed = true;
                clamp_factor = (0x7FFFFFFFi32 - start_x) / sin_interp.0;
            }
            // Check Y overflow: cos > 0 but endpoint wrapped below start
            if cos_interp.0 > 0 && endpoint_y < start_y {
                let y_clamp = (0x7FFFFFFFi32 - start_y) / cos_interp.0;
                if !overflowed || y_clamp < clamp_factor {
                    clamp_factor = y_clamp;
                }
                overflowed = true;
            }
            if overflowed {
                endpoint_x = start_x + clamp_factor * sin_interp.0;
                endpoint_y = start_y + clamp_factor * cos_interp.0;
            }
        }

        // Enqueue polygon line (2 vertices)
        let poly_param_1 = world.gfx_color_table[8]; // crosshair line style
        let poly_param_2 = world.gfx_color_table[6]; // crosshair line color
        let verts: [[i32; 3]; 2] = [[start_x, start_y, 0], [endpoint_x, endpoint_y, 0]];
        let byte_len = 2 * core::mem::size_of::<[i32; 3]>();
        if let Some(vert_ptr) = rq.alloc_aux(byte_len) {
            core::ptr::copy_nonoverlapping(verts.as_ptr() as *const u8, vert_ptr, byte_len);
            let _ = rq.push_typed(
                0xE_0000,
                RenderMessage::Polygon {
                    count: 2,
                    color1: poly_param_1,
                    color2: poly_param_2,
                    vertices: vert_ptr as *const [i32; 3],
                },
            );
        }

        // Draw crosshair sprite at endpoint (only if no overflow clamping)
        if !overflowed {
            let _ = rq.push_typed(
                0x4_0000,
                RenderMessage::Sprite {
                    local: true,
                    x: Fixed(endpoint_x).floor(),
                    y: Fixed(endpoint_y).floor(),
                    sprite: SpriteOp(0x44),
                    palette: (0x8000u32).wrapping_sub(angle),
                },
            );
        }
    }
}
