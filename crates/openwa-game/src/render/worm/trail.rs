//! `WormEntity::DrawTrail` (0x00500720) — draws the segmented rope/grapple
//! polyline anchored to a worm during ninja-rope, bungee, and kamikaze use.
//!
//! The function is misnamed in Ghidra as "DrawBungeeTrail"; there is no
//! separate `BungeeTrailEntity` class. The receiver is a `WormEntity` and the
//! trail state lives in WorldEntity-base slots `+0xBC..+0xE4` that the
//! ninja-rope (state 0x7C) and kamikaze paths share via snap/restore at
//! 0x005003D0 / 0x00500630. Offsets used here:
//!
//! - `+0xBC` (`_field_bc`): trail-active gate (1 = render).
//! - `+0xC0/+0xC4`: anchor (rope hook) coords.
//! - `+0xD0`: segment count.
//! - `+0xE4`: segment buffer (`wa_malloc(0x220)` — 0x40 × 8 bytes; angle at
//!   `[buf+4+i*8]`).
//! - `+0x84/+0x88`: worm position (rope's other endpoint).

use crate::entity::WormEntity;
use crate::render::SpriteOp;
use crate::render::message::RenderMessage;
use openwa_core::fixed::Fixed;
use openwa_core::trig::{cos, sin};

#[inline]
unsafe fn read_i32(this: *const WormEntity, offset: usize) -> i32 {
    unsafe { *((this as *const u8).add(offset) as *const i32) }
}

#[inline]
unsafe fn read_ptr(this: *const WormEntity, offset: usize) -> *const u8 {
    unsafe { *((this as *const u8).add(offset) as *const *const u8) }
}

pub unsafe fn draw_worm_trail(this: *const WormEntity, style: u32, fill: u32) {
    unsafe {
        if read_i32(this, 0xBC) == 0 {
            return;
        }

        let world = &*(*this).base.base.world;
        let rq = &mut *world.render_queue;

        let seg_data = read_ptr(this, 0xE4);
        if seg_data.is_null() {
            return;
        }

        let segment_count = read_i32(this, 0xD0);
        if segment_count <= 0 {
            return;
        }

        let mut x = read_i32(this, 0xC0);
        let mut y = read_i32(this, 0xC4);

        let first_angle = *(seg_data.add(4) as *const i32);

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

        const MAX_VERTICES: usize = 256;
        let mut verts = [[0i32; 3]; MAX_VERTICES];
        let mut vert_count: usize = 0;
        let mut accumulated_angle: u32 = 0;

        for i in 0..segment_count {
            let seg_angle = *(seg_data.add(4 + i as usize * 8) as *const i32);

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

        if vert_count < MAX_VERTICES {
            let pos_x = (*this).base.pos_x.0;
            let pos_y = (*this).base.pos_y.0;
            verts[vert_count] = [pos_x, pos_y, 0];
            vert_count += 1;
        }

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
