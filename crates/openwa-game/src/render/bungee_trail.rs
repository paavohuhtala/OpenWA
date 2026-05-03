//! Bungee trail rendering — draws the bungee drop trajectory path.
//!
//! Ported from WA.exe `DrawBungeeTrail` (0x500720).
//! Draws:
//!   1. Sprite at trail start position
//!   2. Series of vertices computed by accumulating angle + trig interpolation
//!   3. Final vertex at entity position (0x84/0x88)
//!   4. DrawPolygon (if fill != 0) or DrawLineStrip

use crate::entity::BungeeTrailEntity;
use crate::render::SpriteOp;
use crate::render::message::RenderMessage;
use openwa_core::fixed::Fixed;
use openwa_core::trig::{cos, sin};

/// Draw bungee trail for the given entity.
pub unsafe fn draw_bungee_trail(entity: *const BungeeTrailEntity, style: u32, fill: u32) {
    unsafe {
        let entity = &*entity;

        if entity.trail_visible == 0 {
            return;
        }

        let world = &*entity.base.world;
        let rq = &mut *world.render_queue;

        let seg_data = entity.segment_data;
        if seg_data.is_null() {
            return;
        }

        let segment_count = entity.segment_count;
        if segment_count <= 0 {
            return;
        }

        let mut x = entity.trail_start_x;
        let mut y = entity.trail_start_y;

        let first_angle = *(seg_data.add(4) as *const i32);

        // Enqueue start sprite (screen-space)
        let _ = rq.push_typed(
            0xDFFFF,
            RenderMessage::Sprite {
                local: true,
                x: Fixed(x).floor(),
                y: Fixed(y).floor(),
                sprite: SpriteOp(0x45),
                palette: (first_angle + 0x8100) as u32,
            },
        );

        // Build vertex array from trail segments
        const MAX_VERTICES: usize = 256;
        let mut verts = [[0i32; 3]; MAX_VERTICES];
        let mut vert_count: usize = 0;
        let mut accumulated_angle: u32 = 0;

        for i in 0..segment_count {
            let seg_angle = *(seg_data.add(4 + i as usize * 8) as *const i32);

            // Include vertex if: first segment, or segment has nonzero angle, or fill mode
            if (i == 0 || seg_angle != 0 || fill != 0) && vert_count < MAX_VERTICES {
                verts[vert_count] = [x, y, 0];
                vert_count += 1;
            }

            accumulated_angle = accumulated_angle.wrapping_add(seg_angle as u32);

            let sin_interp = sin(accumulated_angle);
            let cos_interp = cos(accumulated_angle);

            x = x.wrapping_add(sin_interp.0.wrapping_mul(8));
            y = y.wrapping_sub(cos_interp.0.wrapping_mul(8));
        }

        // Final vertex = entity position (target)
        if vert_count < MAX_VERTICES {
            verts[vert_count] = [entity.pos_x.0, entity.pos_y.0, 0];
            vert_count += 1;
        }

        // Allocate vertex data in the arena and enqueue as polygon or line strip
        let byte_len = vert_count * core::mem::size_of::<[i32; 3]>();
        if let Some(vert_ptr) = rq.alloc_aux(byte_len) {
            core::ptr::copy_nonoverlapping(verts.as_ptr() as *const u8, vert_ptr, byte_len);
            let vert_slice = vert_ptr as *const [i32; 3];

            if fill != 0 {
                let _ = rq.push_typed(
                    0xE_0000,
                    RenderMessage::Polygon {
                        count: vert_count as u32,
                        color1: style,
                        color2: fill,
                        vertices: vert_slice,
                    },
                );
            } else {
                let _ = rq.push_typed(
                    0xE_0000,
                    RenderMessage::LineStrip {
                        count: vert_count as u32,
                        color: style,
                        vertices: vert_slice,
                    },
                );
            }
        }
    }
}
